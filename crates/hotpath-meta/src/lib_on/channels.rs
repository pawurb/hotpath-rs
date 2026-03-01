//! Channel instrumentation module - tracks message flow, queue sizes, and channel state.

use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::instant::Instant;

mod wrapper;

use std::mem;

use crate::data_flow::{
    next_data_flow_id, WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
use crate::json::{format_queue_status, JsonChannelEntry};
pub(crate) use crate::json::{ChannelLogs, ChannelState, DataFlowLogEntry};
use crate::metrics_server::METRICS_SERVER_PORT;
use crate::output::format_bytes;

pub use crate::Format;

/// Handle returned by [`register_channel`] that gives wrappers the channel's
/// unique id and a sender to emit [`ChannelEvent`]s to the background worker.
pub(crate) struct RegisteredChannel {
    pub(crate) id: u32,
    pub(crate) stats_tx: CbSender<ChannelEvent>,
}

/// Type of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelType {
    Bounded(usize),
    Unbounded,
    Oneshot,
}

/// Registers a new channel with the profiling subsystem.
///
/// Sends a [`ChannelEvent::Created`] event to the background worker and returns
/// a [`RegisteredChannel`] that wrappers use to report subsequent
/// send/receive/close events. `T` is the message type carried by the channel
/// and is used to record the type name and per-message byte size.
pub(crate) fn register_channel<T>(
    source: &'static str,
    label: Option<String>,
    channel_type: ChannelType,
) -> RegisteredChannel {
    let type_name = std::any::type_name::<T>();
    let state = init_channels_state();
    let id = next_data_flow_id();

    let _ = state.event_tx.send(ChannelEvent::Created {
        id,
        source,
        display_label: label,
        channel_type,
        type_name,
        type_size: mem::size_of::<T>(),
    });

    RegisteredChannel {
        id,
        stats_tx: state.event_tx.clone(),
    }
}

pub(crate) fn timestamp_nanos(timestamp: Instant) -> u64 {
    let start_time = START_TIME.get().copied().unwrap_or(timestamp);
    timestamp.duration_since(start_time).as_nanos() as u64
}

/// Statistics for a single instrumented channel.
#[derive(Debug, Clone)]
pub(crate) struct ChannelEntry {
    pub(crate) id: u32,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) channel_type: ChannelType,
    pub(crate) state: ChannelState,
    pub(crate) sent_count: u64,
    pub(crate) received_count: u64,
    pub(crate) type_name: &'static str,
    pub(crate) type_size: usize,
    pub(crate) max_queued: u64,
    pub(crate) iter: u32,
}

#[derive(Debug)]
pub(crate) struct ChannelEntryLogs {
    pub(crate) sent_logs: VecDeque<DataFlowLogEntry>,
    pub(crate) received_logs: VecDeque<DataFlowLogEntry>,
}

impl ChannelEntryLogs {
    fn new() -> Self {
        Self {
            sent_logs: VecDeque::with_capacity(*LOGS_LIMIT),
            received_logs: VecDeque::with_capacity(*LOGS_LIMIT),
        }
    }
}

pub(crate) struct ChannelsInternalState {
    pub(crate) stats: HashMap<u32, ChannelEntry>,
    pub(crate) logs: HashMap<u32, ChannelEntryLogs>,
}

impl ChannelEntry {
    pub(crate) fn queued(&self) -> u64 {
        self.sent_count
            .saturating_sub(self.received_count)
            .saturating_sub(1)
    }

    pub(crate) fn queued_bytes(&self) -> u64 {
        self.queued() * self.type_size as u64
    }
}

impl From<&ChannelEntry> for JsonChannelEntry {
    fn from(stats: &ChannelEntry) -> Self {
        let label = resolve_label(stats.source, stats.label.as_deref(), Some(stats.iter));
        let queued = stats.queued();
        let capacity = match &stats.channel_type {
            ChannelType::Bounded(cap) => Some(cap),
            _ => None,
        };

        JsonChannelEntry {
            id: stats.id,
            source: stats.source.to_string(),
            label,
            has_custom_label: stats.label.is_some(),
            channel_type: stats.channel_type.to_string(),
            state: stats.state.as_str().to_string(),
            sent_count: stats.sent_count,
            received_count: stats.received_count,
            queued,
            max_queued: stats.max_queued,
            queue_status: format_queue_status(queued, capacity.copied()),
            type_name: stats.type_name.to_string(),
            type_size: stats.type_size,
            queued_bytes: format_bytes(stats.queued_bytes()),
            iter: stats.iter,
        }
    }
}

impl ChannelEntry {
    fn new(
        id: u32,
        source: &'static str,
        label: Option<String>,
        channel_type: ChannelType,
        type_name: &'static str,
        type_size: usize,
        iter: u32,
    ) -> Self {
        Self {
            id,
            source,
            label,
            channel_type,
            state: ChannelState::default(),
            sent_count: 0,
            received_count: 0,
            type_name,
            type_size,
            max_queued: 0,
            iter,
        }
    }

    fn update_state(&mut self) {
        let queued = self.queued();
        self.max_queued = self.max_queued.max(queued);

        if self.state == ChannelState::Closed || self.state == ChannelState::Notified {
            return;
        }

        let is_full = match self.channel_type {
            ChannelType::Bounded(cap) => queued >= cap as u64,
            ChannelType::Oneshot => queued >= 1,
            ChannelType::Unbounded => false,
        };

        if is_full {
            self.state = ChannelState::Full;
        } else {
            self.state = ChannelState::Active;
        }
    }
}

/// Events sent to the background channel statistics collection thread.
#[derive(Debug)]
pub(crate) enum ChannelEvent {
    Created {
        id: u32,
        source: &'static str,
        display_label: Option<String>,
        channel_type: ChannelType,
        type_name: &'static str,
        type_size: usize,
    },
    MessageSent {
        id: u32,
        log: Option<String>,
        timestamp: Instant,
    },
    MessageReceived {
        id: u32,
        timestamp: Instant,
    },
    Closed {
        id: u32,
    },
    #[allow(dead_code)]
    Notified {
        id: u32,
    },
}

pub(crate) struct ChannelsState {
    pub(crate) event_tx: CbSender<ChannelEvent>,
    pub(crate) inner: Arc<RwLock<ChannelsInternalState>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<()>>>,
}

type ChannelStatsState = ChannelsState;

pub(crate) static CHANNELS_STATE: OnceLock<ChannelStatsState> = OnceLock::new();

pub(crate) use crate::lib_on::START_TIME;

pub(crate) use crate::lib_on::hotpath_guard::LOGS_LIMIT;

fn process_channel_event(state: &mut ChannelsInternalState, event: ChannelEvent) {
    match event {
        ChannelEvent::Created {
            id,
            source,
            display_label,
            channel_type,
            type_name,
            type_size,
        } => {
            let iter = state.stats.values().filter(|s| s.source == source).count() as u32;
            state.stats.insert(
                id,
                ChannelEntry::new(
                    id,
                    source,
                    display_label,
                    channel_type,
                    type_name,
                    type_size,
                    iter,
                ),
            );
            state.logs.insert(id, ChannelEntryLogs::new());
        }
        ChannelEvent::MessageSent { id, log, timestamp } => {
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.sent_count += 1;
                channel_stats.update_state();
            }
            if let Some(entry_logs) = state.logs.get_mut(&id) {
                let sent_count = state.stats.get(&id).map_or(0, |s| s.sent_count);
                let limit = *LOGS_LIMIT;
                if entry_logs.sent_logs.len() >= limit {
                    entry_logs.sent_logs.pop_front();
                }
                entry_logs.sent_logs.push_back(DataFlowLogEntry::new(
                    sent_count,
                    timestamp_nanos(timestamp),
                    log,
                    None,
                ));
            }
        }
        ChannelEvent::MessageReceived { id, timestamp } => {
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.received_count += 1;
                channel_stats.update_state();
            }
            if let Some(entry_logs) = state.logs.get_mut(&id) {
                let received_count = state.stats.get(&id).map_or(0, |s| s.received_count);
                let limit = *LOGS_LIMIT;
                if entry_logs.received_logs.len() >= limit {
                    entry_logs.received_logs.pop_front();
                }
                entry_logs.received_logs.push_back(DataFlowLogEntry::new(
                    received_count,
                    timestamp_nanos(timestamp),
                    None,
                    None,
                ));
            }
        }
        ChannelEvent::Closed { id } => {
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.state = ChannelState::Closed;
            }
        }
        ChannelEvent::Notified { id } => {
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.state = ChannelState::Notified;
            }
        }
    }
}

/// Initialize the channel statistics collection system (called on first instrumented channel).
pub(crate) fn init_channels_state() -> &'static ChannelStatsState {
    CHANNELS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (event_tx, event_rx) = unbounded::<ChannelEvent>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);
        let inner = Arc::new(RwLock::new(ChannelsInternalState {
            stats: HashMap::new(),
            logs: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("hp-meta-channels".into())
            .spawn(move || {
                let mut local_buffer: Vec<ChannelEvent> = Vec::with_capacity(WORKER_BATCH_SIZE);
                let flush_interval = std::time::Duration::from_millis(WORKER_FLUSH_INTERVAL_MS);

                loop {
                    select! {
                        recv(event_rx) -> result => {
                            match result {
                                Ok(event) => {
                                    local_buffer.push(event);
                                    if local_buffer.len() >= WORKER_BATCH_SIZE {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_channel_event(&mut shared, e);
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    if !local_buffer.is_empty() {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_channel_event(&mut shared, e);
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                        recv(shutdown_rx) -> _ => {
                            let mut drained_events = Vec::with_capacity(WORKER_BATCH_SIZE);
                            for _ in 0..WORKER_SHUTDOWN_DRAIN_LIMIT {
                                match event_rx.try_recv() {
                                    Ok(event) => drained_events.push(event),
                                    Err(_) => break,
                                }
                            }

                            if let Ok(mut shared) = inner_clone.write() {
                                for e in local_buffer.drain(..) {
                                    process_channel_event(&mut shared, e);
                                }
                                for event in drained_events {
                                    process_channel_event(&mut shared, event);
                                }
                            }
                            break;
                        }
                        default(flush_interval) => {
                            if !local_buffer.is_empty() {
                                if let Ok(mut shared) = inner_clone.write() {
                                    for e in local_buffer.drain(..) {
                                        process_channel_event(&mut shared, e);
                                    }
                                }
                            }
                        }
                    }
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn channel-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        ChannelsState {
            event_tx,
            inner,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
    })
}

pub(crate) fn resolve_label(id: &'static str, provided: Option<&str>, iter: Option<u32>) -> String {
    let base_label = if let Some(l) = provided {
        l.to_string()
    } else if let Some(pos) = id.rfind(':') {
        let (path, line_part) = id.split_at(pos);
        let line = &line_part[1..];
        format!("{}:{}", extract_filename(path), line)
    } else {
        extract_filename(id)
    };

    match iter {
        Some(i) if i > 0 => format!("{}-{}", base_label, i + 1),
        _ => base_label,
    }
}

pub(crate) fn extract_filename(path: &str) -> String {
    let components: Vec<&str> = path.split('/').collect();
    if components.len() >= 2 {
        format!(
            "{}/{}",
            components[components.len() - 2],
            components[components.len() - 1]
        )
    } else {
        path.to_string()
    }
}

/// Trait for instrumenting channels.
///
/// This trait is not intended for direct use. Use the `channel!` macro instead.
#[doc(hidden)]
pub trait InstrumentChannel {
    type Output;
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output;
}

/// Trait for instrumenting channels with message logging.
///
/// This trait is not intended for direct use. Use the `channel!` macro with `log = true` instead.
#[doc(hidden)]
pub trait InstrumentChannelLog {
    type Output;
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output;
}

cfg_if::cfg_if! {
    if #[cfg(any(feature = "tokio", feature = "futures"))] {
        pub(crate) static RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_time()
                .build()
                .unwrap()
        });
    }
}

/// Instrument a channel creation to wrap it with debugging proxies.
///
/// # Examples
///
/// ```rust,no_run
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///    let (tx, rx) = hotpath_meta::channel!(mpsc::channel::<String>(100));
///
///    tx.send("Hello".to_string()).await.unwrap();
/// }
/// ```
#[macro_export]
macro_rules! channel {
    ($expr:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannel::instrument($expr, CHANNEL_ID, None, None)
    }};

    ($expr:expr, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannel::instrument($expr, CHANNEL_ID, Some($label.to_string()), None)
    }};

    ($expr:expr, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannel::instrument($expr, CHANNEL_ID, None, Some($capacity))
    }};

    ($expr:expr, label = $label:expr, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannel::instrument(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, capacity = $capacity:expr, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannel::instrument(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    // Variants with log = true
    ($expr:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelLog::instrument_log($expr, CHANNEL_ID, None, None)
    }};

    ($expr:expr, label = $label:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, log = true, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, capacity = $capacity:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log($expr, CHANNEL_ID, None, Some($capacity))
    }};

    ($expr:expr, log = true, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log($expr, CHANNEL_ID, None, Some($capacity))
    }};

    ($expr:expr, label = $label:expr, capacity = $capacity:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, label = $label:expr, log = true, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, capacity = $capacity:expr, label = $label:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, capacity = $capacity:expr, log = true, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, log = true, label = $label:expr, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, log = true, capacity = $capacity:expr, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};
}

/// Compare two channel stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source and iter).
pub(crate) fn compare_channel_entries(a: &ChannelEntry, b: &ChannelEntry) -> std::cmp::Ordering {
    let a_has_label = a.label.is_some();
    let b_has_label = b.label.is_some();

    match (a_has_label, b_has_label) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a
            .label
            .as_ref()
            .unwrap()
            .cmp(b.label.as_ref().unwrap())
            .then_with(|| a.iter.cmp(&b.iter)),
        (false, false) => a.source.cmp(b.source).then_with(|| a.iter.cmp(&b.iter)),
    }
}

pub(crate) fn get_sorted_channel_entries() -> Vec<ChannelEntry> {
    let Some(state) = CHANNELS_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<ChannelEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_channel_entries);
    stats
}

pub(crate) fn get_channel_logs(channel_id: &str) -> Option<ChannelLogs> {
    let id = channel_id.parse::<u32>().ok()?;
    let state = CHANNELS_STATE.get()?;
    let guard = state.inner.read().unwrap();
    let entry_logs = guard.logs.get(&id)?;
    Some(ChannelLogs {
        id: channel_id.to_string(),
        sent_logs: entry_logs.sent_logs.iter().rev().cloned().collect(),
        received_logs: entry_logs.received_logs.iter().rev().cloned().collect(),
    })
}
