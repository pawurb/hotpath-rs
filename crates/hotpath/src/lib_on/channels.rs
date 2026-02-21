//! Channel instrumentation module - tracks message flow, queue sizes, and channel state.

use crossbeam_channel::{
    bounded, select_biased, unbounded, Receiver as CbReceiver, Sender as CbSender,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

mod wrapper;

use std::mem;

use crate::data_flow::next_data_flow_id;
use crate::json::{format_queue_status, JsonChannelEntry, JsonChannelsList};
pub use crate::json::{ChannelLogs, ChannelState, DataFlowLogEntry};
use crate::metrics_server::{METRICS_SERVER_PORT, RECV_TIMEOUT_MS};
use crate::output::format_bytes;

pub use crate::Format;

/// Handle returned by [`register_channel`] that gives wrappers the channel's
/// unique id and a sender to emit [`ChannelEvent`]s to the background worker.
pub struct RegisteredChannel {
    pub id: u32,
    pub stats_tx: CbSender<ChannelEvent>,
}

/// Type of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
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
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
pub fn register_channel<T>(
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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
    pub(crate) sent_logs: VecDeque<DataFlowLogEntry>,
    pub(crate) received_logs: VecDeque<DataFlowLogEntry>,
    pub(crate) iter: u32,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl ChannelEntry {
    pub fn queued(&self) -> u64 {
        self.sent_count
            .saturating_sub(self.received_count)
            .saturating_sub(1)
    }

    pub fn queued_bytes(&self) -> u64 {
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
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
            sent_logs: VecDeque::new(),
            received_logs: VecDeque::new(),
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
pub enum ChannelEvent {
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
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<HashMap<u32, ChannelEntry>>>>,
}

type ChannelStatsState = ChannelsState;

pub(crate) static CHANNELS_STATE: OnceLock<ChannelStatsState> = OnceLock::new();

pub(crate) static CHANNELS_QUERY_TX: OnceLock<CbSender<ChannelsQuery>> = OnceLock::new();

#[derive(Debug)]
pub(crate) enum ChannelsQuery {
    SortedEntries(CbSender<Vec<ChannelEntry>>),
    Json(CbSender<JsonChannelsList>),
    Logs {
        channel_id: u32,
        response_tx: CbSender<Option<ChannelLogs>>,
    },
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
fn query_channels_state<T, F>(make_query: F) -> Option<T>
where
    F: FnOnce(CbSender<T>) -> ChannelsQuery,
{
    let query_tx = CHANNELS_QUERY_TX.get()?;
    let (response_tx, response_rx) = bounded::<T>(1);
    query_tx.send(make_query(response_tx)).ok()?;
    response_rx
        .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
        .ok()
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn handle_channel_query(query: ChannelsQuery, local_stats: &HashMap<u32, ChannelEntry>) {
    match query {
        ChannelsQuery::SortedEntries(response_tx) => {
            let mut entries: Vec<ChannelEntry> = local_stats.values().cloned().collect();
            entries.sort_by(compare_channel_entries);
            let _ = response_tx.send(entries);
        }
        ChannelsQuery::Json(response_tx) => {
            let mut entries: Vec<ChannelEntry> = local_stats.values().cloned().collect();
            entries.sort_by(compare_channel_entries);
            let data = entries.iter().map(JsonChannelEntry::from).collect();
            let current_elapsed_ns = START_TIME
                .get()
                .map(|t| t.elapsed().as_nanos() as u64)
                .unwrap_or(0);
            let _ = response_tx.send(JsonChannelsList {
                current_elapsed_ns,
                data,
            });
        }
        ChannelsQuery::Logs {
            channel_id,
            response_tx,
        } => {
            let result = local_stats.get(&channel_id).map(|stats| ChannelLogs {
                id: channel_id.to_string(),
                sent_logs: stats.sent_logs.iter().rev().cloned().collect(),
                received_logs: stats.received_logs.iter().rev().cloned().collect(),
            });
            let _ = response_tx.send(result);
        }
    }
}

pub(crate) use crate::lib_on::START_TIME;

pub(crate) use crate::lib_on::hotpath_guard::LOGS_LIMIT;

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn process_channel_event(stats: &mut HashMap<u32, ChannelEntry>, event: ChannelEvent) {
    match event {
        ChannelEvent::Created {
            id,
            source,
            display_label,
            channel_type,
            type_name,
            type_size,
        } => {
            let iter = stats.values().filter(|s| s.source == source).count() as u32;
            stats.insert(
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
        }
        ChannelEvent::MessageSent { id, log, timestamp } => {
            if let Some(channel_stats) = stats.get_mut(&id) {
                channel_stats.sent_count += 1;
                channel_stats.update_state();

                let limit = *LOGS_LIMIT;
                if channel_stats.sent_logs.len() >= limit {
                    channel_stats.sent_logs.pop_front();
                }
                channel_stats.sent_logs.push_back(DataFlowLogEntry::new(
                    channel_stats.sent_count,
                    timestamp_nanos(timestamp),
                    log,
                    None,
                ));
            }
        }
        ChannelEvent::MessageReceived { id, timestamp } => {
            if let Some(channel_stats) = stats.get_mut(&id) {
                channel_stats.received_count += 1;
                channel_stats.update_state();

                let limit = *LOGS_LIMIT;
                if channel_stats.received_logs.len() >= limit {
                    channel_stats.received_logs.pop_front();
                }
                channel_stats.received_logs.push_back(DataFlowLogEntry::new(
                    channel_stats.received_count,
                    timestamp_nanos(timestamp),
                    None,
                    None,
                ));
            }
        }
        ChannelEvent::Closed { id } => {
            if let Some(channel_stats) = stats.get_mut(&id) {
                channel_stats.state = ChannelState::Closed;
            }
        }
        ChannelEvent::Notified { id } => {
            if let Some(channel_stats) = stats.get_mut(&id) {
                channel_stats.state = ChannelState::Notified;
            }
        }
    }
}

/// Initialize the channel statistics collection system (called on first instrumented channel).
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
pub(crate) fn init_channels_state() -> &'static ChannelStatsState {
    CHANNELS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (event_tx, event_rx) = unbounded::<ChannelEvent>();
        #[cfg(feature = "hotpath-meta")]
        let (event_tx, event_rx) =
            hotpath_meta::channel!((event_tx, event_rx), label = "hp-ch-events", log = true);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        #[cfg(feature = "hotpath-meta")]
        let (shutdown_tx, shutdown_rx) = hotpath_meta::channel!(
            (shutdown_tx, shutdown_rx),
            label = "hp-ch-shutdown",
            log = true
        );
        let (completion_tx, completion_rx) = bounded::<HashMap<u32, ChannelEntry>>(1);
        #[cfg(feature = "hotpath-meta")]
        let (completion_tx, completion_rx) = hotpath_meta::channel!(
            (completion_tx, completion_rx),
            label = "hp-ch-completion",
            log = true
        );
        let (query_tx, query_rx) = unbounded::<ChannelsQuery>();
        #[cfg(feature = "hotpath-meta")]
        let (query_tx, query_rx) =
            hotpath_meta::channel!((query_tx, query_rx), label = "hp-ch-queries", log = true);
        let _ = CHANNELS_QUERY_TX.set(query_tx);

        std::thread::Builder::new()
            .name("hp-channels".into())
            .spawn(move || {
                let mut local_stats = HashMap::<u32, ChannelEntry>::new();

                loop {
                    select_biased! {
                        recv(shutdown_rx) -> _ => {
                            while let Ok(event) = event_rx.try_recv() {
                                process_channel_event(&mut local_stats, event);
                            }
                            break;
                        }
                        recv(query_rx) -> result => {
                            if let Ok(query) = result {
                                handle_channel_query(query, &local_stats);
                            }
                        }
                        recv(event_rx) -> result => {
                            match result {
                                Ok(event) => {
                                    process_channel_event(&mut local_stats, event);
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }

                let _ = completion_tx.send(local_stats);
            })
            .expect("Failed to spawn channel-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        ChannelsState {
            event_tx,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
    })
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn extract_filename(path: &str) -> String {
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
        pub static RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_time()
                .build()
                .unwrap()
        });
    }
}

/// Instrument a channel creation to wrap it with debugging proxies.
/// Currently only supports bounded, unbounded and oneshot channels.
///
/// # Examples
///
/// ```
/// use tokio::sync::mpsc;
/// use channels_console::channel;
///
/// #[tokio::main]
/// async fn main() {
///    // Create channels normally
///    let (tx, rx) = mpsc::channel::<String>(100);
///
///    // Instrument them only when the feature is enabled
///    #[cfg(feature = "hotpath")]
///    let (tx, rx) = channels_console::channel!((tx, rx));
///
///    // The channel works exactly the same way
///    tx.send("Hello".to_string()).await.unwrap();
/// }
/// ```
///
/// See the `channel!` macro documentation for full usage details.
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
        $crate::InstrumentLog::instrument_log(
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
        $crate::InstrumentLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, log = true, label = $label:expr, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, log = true, capacity = $capacity:expr, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentLog::instrument_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_sorted_channel_entries() -> Vec<ChannelEntry> {
    query_channels_state(ChannelsQuery::SortedEntries).unwrap_or_default()
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn get_channels_json() -> JsonChannelsList {
    if let Some(json) = query_channels_state(ChannelsQuery::Json) {
        return json;
    }

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    JsonChannelsList {
        current_elapsed_ns,
        data: Vec::new(),
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn get_channel_logs(channel_id: &str) -> Option<ChannelLogs> {
    let id = channel_id.parse::<u32>().ok()?;
    query_channels_state(|response_tx| ChannelsQuery::Logs {
        channel_id: id,
        response_tx,
    })
    .flatten()
}
