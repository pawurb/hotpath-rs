//! Channel instrumentation module - tracks message flow, queue sizes, and channel state.

use crossbeam_channel::{unbounded, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod guard;
pub use guard::{ChannelsGuard, ChannelsGuardBuilder};

mod wrapper;

use crate::http_server::HTTP_SERVER_PORT;
pub use crate::json::{
    ChannelLogs, ChannelState, ChannelType, ChannelsJson, LogEntry, SerializableChannelStats,
};
use crate::output::truncate_result;

pub use crate::Format;

pub(crate) fn timestamp_nanos(timestamp: Instant) -> u64 {
    let start_time = START_TIME.get().copied().unwrap_or(timestamp);
    timestamp.duration_since(start_time).as_nanos() as u64
}

/// Statistics for a single instrumented channel.
#[derive(Debug, Clone)]
pub(crate) struct ChannelStats {
    pub(crate) id: u64,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) channel_type: ChannelType,
    pub(crate) state: ChannelState,
    pub(crate) sent_count: u64,
    pub(crate) received_count: u64,
    pub(crate) type_name: &'static str,
    pub(crate) type_size: usize,
    pub(crate) sent_logs: VecDeque<LogEntry>,
    pub(crate) received_logs: VecDeque<LogEntry>,
    pub(crate) iter: u32,
}

impl ChannelStats {
    pub fn queued(&self) -> u64 {
        self.sent_count
            .saturating_sub(self.received_count)
            .saturating_sub(1)
    }

    pub fn queued_bytes(&self) -> u64 {
        self.queued() * self.type_size as u64
    }
}

impl From<&ChannelStats> for SerializableChannelStats {
    fn from(channel_stats: &ChannelStats) -> Self {
        let label = resolve_label(
            channel_stats.source,
            channel_stats.label.as_deref(),
            Some(channel_stats.iter),
        );

        Self {
            id: channel_stats.id,
            source: channel_stats.source.to_string(),
            label,
            has_custom_label: channel_stats.label.is_some(),
            channel_type: channel_stats.channel_type,
            state: channel_stats.state,
            sent_count: channel_stats.sent_count,
            received_count: channel_stats.received_count,
            queued: channel_stats.queued(),
            type_name: channel_stats.type_name.to_string(),
            type_size: channel_stats.type_size,
            queued_bytes: channel_stats.queued_bytes(),
            iter: channel_stats.iter,
        }
    }
}

impl ChannelStats {
    fn new(
        id: u64,
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
            sent_logs: VecDeque::new(),
            received_logs: VecDeque::new(),
            iter,
        }
    }

    fn update_state(&mut self) {
        if self.state == ChannelState::Closed || self.state == ChannelState::Notified {
            return;
        }

        let queued = self.queued();
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
        id: u64,
        source: &'static str,
        display_label: Option<String>,
        channel_type: ChannelType,
        type_name: &'static str,
        type_size: usize,
    },
    MessageSent {
        id: u64,
        log: Option<String>,
        timestamp: Instant,
    },
    MessageReceived {
        id: u64,
        timestamp: Instant,
    },
    Closed {
        id: u64,
    },
    #[allow(dead_code)]
    Notified {
        id: u64,
    },
}

type ChannelStatsState = (
    CbSender<ChannelEvent>,
    Arc<RwLock<HashMap<u64, ChannelStats>>>,
);

static CHANNELS_STATE: OnceLock<ChannelStatsState> = OnceLock::new();

pub(crate) static START_TIME: OnceLock<Instant> = OnceLock::new();

pub(crate) static CHANNEL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

const DEFAULT_LOG_LIMIT: usize = 50;

pub(crate) fn get_log_limit() -> usize {
    std::env::var("HOTPATH_LOGS_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LOG_LIMIT)
}

/// Initialize the channel statistics collection system (called on first instrumented channel).
pub(crate) fn init_channels_state() -> &'static ChannelStatsState {
    CHANNELS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (tx, rx) = unbounded::<ChannelEvent>();
        let stats_map = Arc::new(RwLock::new(HashMap::<u64, ChannelStats>::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-channels".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    let mut stats = stats_map_clone.write().unwrap();
                    match event {
                        ChannelEvent::Created {
                            id,
                            source,
                            display_label,
                            channel_type,
                            type_name,
                            type_size,
                        } => {
                            // Count existing items with the same source location
                            let iter = stats.values().filter(|s| s.source == source).count() as u32;

                            stats.insert(
                                id,
                                ChannelStats::new(
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

                                let limit = get_log_limit();
                                if channel_stats.sent_logs.len() >= limit {
                                    channel_stats.sent_logs.pop_front();
                                }
                                channel_stats.sent_logs.push_back(LogEntry::new(
                                    channel_stats.sent_count,
                                    timestamp_nanos(timestamp),
                                    log.map(truncate_result),
                                    None,
                                ));
                            }
                        }
                        ChannelEvent::MessageReceived { id, timestamp } => {
                            if let Some(channel_stats) = stats.get_mut(&id) {
                                channel_stats.received_count += 1;
                                channel_stats.update_state();

                                let limit = get_log_limit();
                                if channel_stats.received_logs.len() >= limit {
                                    channel_stats.received_logs.pop_front();
                                }
                                channel_stats.received_logs.push_back(LogEntry::new(
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
            })
            .expect("Failed to spawn channel-stats-collector thread");

        crate::http_server::start_metrics_server_once(*HTTP_SERVER_PORT);

        (tx, stats_map)
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
        use std::sync::LazyLock;
        pub static RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
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

fn get_all_channel_stats() -> HashMap<u64, ChannelStats> {
    if let Some((_, stats_map)) = CHANNELS_STATE.get() {
        stats_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

/// Compare two channel stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source and iter).
fn compare_channel_stats(a: &ChannelStats, b: &ChannelStats) -> std::cmp::Ordering {
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

pub(crate) fn get_sorted_channel_stats() -> Vec<ChannelStats> {
    let mut stats: Vec<ChannelStats> = get_all_channel_stats().into_values().collect();
    stats.sort_by(compare_channel_stats);
    stats
}

pub fn get_channels_json() -> ChannelsJson {
    let channels = get_sorted_channel_stats()
        .iter()
        .map(SerializableChannelStats::from)
        .collect();

    let current_elapsed_ns = START_TIME
        .get()
        .expect("START_TIME must be initialized")
        .elapsed()
        .as_nanos() as u64;

    ChannelsJson {
        current_elapsed_ns,
        channels,
    }
}

pub fn get_channel_logs(channel_id: &str) -> Option<ChannelLogs> {
    let id = channel_id.parse::<u64>().ok()?;
    let stats = get_all_channel_stats();
    stats.get(&id).map(|channel_stats| ChannelLogs {
        id: channel_id.to_string(),
        sent_logs: channel_stats.sent_logs.iter().rev().cloned().collect(),
        received_logs: channel_stats.received_logs.iter().rev().cloned().collect(),
    })
}
