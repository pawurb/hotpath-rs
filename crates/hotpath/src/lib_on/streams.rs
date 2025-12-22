//! Stream instrumentation module - tracks items yielded and stream lifecycle.

use crossbeam_channel::{unbounded, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod guard;
pub use guard::{StreamsGuard, StreamsGuardBuilder};

pub(crate) mod wrapper;

use crate::http_server::HTTP_SERVER_PORT;
pub use crate::json::{ChannelState, LogEntry, SerializableStreamStats, StreamLogs, StreamsJson};
pub use crate::Format;

/// Statistics for a single instrumented stream.
#[derive(Debug, Clone)]
pub(crate) struct StreamStats {
    pub(crate) id: u64,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) state: ChannelState, // Only Active or Closed
    pub(crate) items_yielded: u64,
    pub(crate) type_name: &'static str,
    pub(crate) type_size: usize,
    pub(crate) logs: VecDeque<LogEntry>,
    pub(crate) iter: u32,
}

impl From<&StreamStats> for SerializableStreamStats {
    fn from(stream_stats: &StreamStats) -> Self {
        let label = crate::channels::resolve_label(
            stream_stats.source,
            stream_stats.label.as_deref(),
            Some(stream_stats.iter),
        );

        Self {
            id: stream_stats.id,
            source: stream_stats.source.to_string(),
            label,
            has_custom_label: stream_stats.label.is_some(),
            state: stream_stats.state,
            items_yielded: stream_stats.items_yielded,
            type_name: stream_stats.type_name.to_string(),
            type_size: stream_stats.type_size,
            iter: stream_stats.iter,
        }
    }
}

impl StreamStats {
    fn new(
        id: u64,
        source: &'static str,
        label: Option<String>,
        type_name: &'static str,
        type_size: usize,
        iter: u32,
    ) -> Self {
        Self {
            id,
            source,
            label,
            state: ChannelState::Active,
            items_yielded: 0,
            type_name,
            type_size,
            logs: VecDeque::new(),
            iter,
        }
    }
}

/// Events sent to the background stream statistics collection thread.
#[derive(Debug)]
pub(crate) enum StreamEvent {
    Created {
        id: u64,
        source: &'static str,
        display_label: Option<String>,
        type_name: &'static str,
        type_size: usize,
    },
    Yielded {
        id: u64,
        log: Option<String>,
        timestamp: Instant,
    },
    Completed {
        id: u64,
    },
}

pub(crate) type StreamStatsState = (
    CbSender<StreamEvent>,
    Arc<RwLock<HashMap<u64, StreamStats>>>,
);

static STREAMS_STATE: OnceLock<StreamStatsState> = OnceLock::new();

pub(crate) static STREAM_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Initialize the stream statistics collection system (called on first instrumented stream).
/// Returns a reference to the global state.
pub(crate) fn init_streams_state() -> &'static StreamStatsState {
    STREAMS_STATE.get_or_init(|| {
        crate::channels::START_TIME.get_or_init(Instant::now);

        let (tx, rx) = unbounded::<StreamEvent>();
        let stats_map = Arc::new(RwLock::new(HashMap::<u64, StreamStats>::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-streams".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    let mut stats = stats_map_clone.write().unwrap();
                    match event {
                        StreamEvent::Created {
                            id,
                            source,
                            display_label,
                            type_name,
                            type_size,
                        } => {
                            // Count existing items with the same source location
                            let iter = stats.values().filter(|s| s.source == source).count() as u32;

                            stats.insert(
                                id,
                                StreamStats::new(
                                    id,
                                    source,
                                    display_label,
                                    type_name,
                                    type_size,
                                    iter,
                                ),
                            );
                        }
                        StreamEvent::Yielded { id, log, timestamp } => {
                            if let Some(stream_stats) = stats.get_mut(&id) {
                                stream_stats.items_yielded += 1;

                                let limit = crate::channels::get_log_limit();
                                if stream_stats.logs.len() >= limit {
                                    stream_stats.logs.pop_front();
                                }
                                stream_stats.logs.push_back(LogEntry::new(
                                    stream_stats.items_yielded,
                                    crate::channels::timestamp_nanos(timestamp),
                                    log,
                                    None,
                                ));
                            }
                        }
                        StreamEvent::Completed { id } => {
                            if let Some(stream_stats) = stats.get_mut(&id) {
                                stream_stats.state = ChannelState::Closed;
                            }
                        }
                    }
                }
            })
            .expect("Failed to spawn stream-stats-collector thread");

        crate::http_server::start_metrics_server_once(*HTTP_SERVER_PORT);

        (tx, stats_map)
    })
}

/// Trait for instrumenting streams.
///
/// This trait is not intended for direct use. Use the `stream!` macro instead.
#[doc(hidden)]
pub trait InstrumentStream {
    type Output;
    fn instrument_stream(self, source: &'static str, label: Option<String>) -> Self::Output;
}

/// Trait for instrumenting streams with message logging.
///
/// This trait is not intended for direct use. Use the `stream!` macro with `log = true` instead.
#[doc(hidden)]
pub trait InstrumentStreamLog {
    type Output;
    fn instrument_stream_log(self, source: &'static str, label: Option<String>) -> Self::Output;
}

// Implement InstrumentStream for all Stream types
impl<S> InstrumentStream for S
where
    S: futures_util::Stream,
{
    type Output = crate::streams::wrapper::InstrumentedStream<S>;

    fn instrument_stream(self, source: &'static str, label: Option<String>) -> Self::Output {
        crate::streams::wrapper::InstrumentedStream::new(self, source, label)
    }
}

// Implement InstrumentStreamLog for all Stream types with Debug items
impl<S> InstrumentStreamLog for S
where
    S: futures_util::Stream,
    S::Item: std::fmt::Debug,
{
    type Output = crate::streams::wrapper::InstrumentedStreamLog<S>;

    fn instrument_stream_log(self, source: &'static str, label: Option<String>) -> Self::Output {
        crate::streams::wrapper::InstrumentedStreamLog::new(self, source, label)
    }
}

/// Instrument a stream to track its item yields.
///
/// # Examples
///
/// ```rust,ignore
/// use futures::stream::{self, StreamExt};
/// use streams_console::stream;
///
/// #[tokio::main]
/// async fn main() {
///     // Create a stream
///     let s = stream::iter(1..=10);
///
///     // Instrument it
///     let s = stream!(s);
///
///     // Use it normally
///     let _items: Vec<_> = s.collect().await;
/// }
/// ```
///
/// See the `stream!` macro documentation for full usage details.
#[macro_export]
macro_rules! stream {
    ($expr:expr) => {{
        const STREAM_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentStream::instrument_stream($expr, STREAM_ID, None)
    }};

    ($expr:expr, label = $label:expr) => {{
        const STREAM_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentStream::instrument_stream($expr, STREAM_ID, Some($label.to_string()))
    }};

    ($expr:expr, log = true) => {{
        const STREAM_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentStreamLog::instrument_stream_log($expr, STREAM_ID, None)
    }};

    ($expr:expr, label = $label:expr, log = true) => {{
        const STREAM_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentStreamLog::instrument_stream_log(
            $expr,
            STREAM_ID,
            Some($label.to_string()),
        )
    }};

    ($expr:expr, log = true, label = $label:expr) => {{
        const STREAM_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentStreamLog::instrument_stream_log(
            $expr,
            STREAM_ID,
            Some($label.to_string()),
        )
    }};
}

fn get_all_stream_stats() -> HashMap<u64, StreamStats> {
    if let Some((_, stats_map)) = STREAMS_STATE.get() {
        stats_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

/// Compare two stream stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source and iter).
fn compare_stream_stats(a: &StreamStats, b: &StreamStats) -> std::cmp::Ordering {
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

pub(crate) fn get_sorted_stream_stats() -> Vec<StreamStats> {
    let mut stats: Vec<StreamStats> = get_all_stream_stats().into_values().collect();
    stats.sort_by(compare_stream_stats);
    stats
}

pub fn get_streams_json() -> StreamsJson {
    let streams = get_sorted_stream_stats()
        .iter()
        .map(SerializableStreamStats::from)
        .collect();

    let current_elapsed_ns = crate::channels::START_TIME
        .get()
        .expect("START_TIME must be initialized")
        .elapsed()
        .as_nanos() as u64;

    StreamsJson {
        current_elapsed_ns,
        streams,
    }
}

pub fn get_stream_logs(stream_id: &str) -> Option<StreamLogs> {
    let id = stream_id.parse::<u64>().ok()?;
    let stats = get_all_stream_stats();
    stats.get(&id).map(|stream_stats| {
        let mut yielded_logs: Vec<LogEntry> = stream_stats.logs.iter().cloned().collect();

        // Sort by index descending (most recent first)
        yielded_logs.sort_by(|a, b| b.index.cmp(&a.index));

        StreamLogs {
            id: stream_id.to_string(),
            logs: yielded_logs,
        }
    })
}
