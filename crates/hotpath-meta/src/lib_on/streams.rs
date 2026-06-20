//! Stream instrumentation module - tracks items yielded and stream lifecycle.

use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::instant::Instant;

pub(crate) mod wrapper;

use crate::batch::{register_thread_batch, BatchRegistry, BatchedMeasurement, MeasurementBatch};
use crate::channels::{resolve_label, LOGS_LIMIT};
use crate::json::JsonStreamEntry;
pub(crate) use crate::json::{ChannelState, DataFlowLogEntry, StreamLogs};
use crate::lib_on::hotpath_guard::{
    WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
use crate::metrics_server::METRICS_SERVER_PORT;
pub use crate::Format;

static STREAM_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub(crate) fn next_stream_id() -> u32 {
    STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Statistics for a single instrumented stream.
#[derive(Debug, Clone)]
pub(crate) struct StreamStats {
    pub(crate) id: u32,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) state: ChannelState, // Only Active or Closed
    pub(crate) items_yielded: u64,
    pub(crate) type_name: &'static str,
    pub(crate) type_size: usize,
    pub(crate) iter: u32,
}

impl StreamStats {
    fn new(
        id: u32,
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
            iter,
        }
    }
}

#[derive(Debug)]
pub(crate) struct StreamStatsLogs {
    pub(crate) logs: VecDeque<DataFlowLogEntry>,
}

impl StreamStatsLogs {
    fn new() -> Self {
        Self {
            logs: VecDeque::with_capacity(*LOGS_LIMIT),
        }
    }
}

pub(crate) struct StreamsInternalState {
    pub(crate) stats: HashMap<u32, StreamStats>,
    pub(crate) logs: HashMap<u32, StreamStatsLogs>,
}

impl From<&StreamStats> for JsonStreamEntry {
    fn from(stats: &StreamStats) -> Self {
        let label = resolve_label(stats.source, stats.label.as_deref(), Some(stats.iter));

        JsonStreamEntry {
            id: stats.id,
            source: stats.source.to_string(),
            label,
            has_custom_label: stats.label.is_some(),
            state: stats.state.as_str().to_string(),
            items_yielded: stats.items_yielded,
            type_name: stats.type_name.to_string(),
            type_size: stats.type_size,
            iter: stats.iter,
        }
    }
}

/// Events sent to the background stream statistics collection thread.
#[derive(Debug)]
pub(crate) enum StreamEvent {
    Created {
        id: u32,
        source: &'static str,
        display_label: Option<String>,
        type_name: &'static str,
        type_size: usize,
    },
    Yielded {
        id: u32,
        log: Option<String>,
        timestamp: Instant,
    },
    Completed {
        id: u32,
    },
}

pub(crate) struct StreamsState {
    pub(crate) event_tx: CbSender<Vec<StreamEvent>>,
    pub(crate) inner: Arc<RwLock<StreamsInternalState>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<()>>>,
}

pub(crate) static STREAMS_STATE: OnceLock<StreamsState> = OnceLock::new();

static EVENT_REGISTRY: BatchRegistry<StreamEvent> = BatchRegistry::new();

thread_local! {
    static EVENT_BATCH: std::sync::Arc<std::sync::Mutex<MeasurementBatch<StreamEvent>>> =
        register_thread_batch(&EVENT_REGISTRY);
}

#[inline]
pub(crate) fn send_stream_event(event: StreamEvent) {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    EVENT_BATCH.with(|b| {
        if let Ok(mut b) = b.lock() {
            b.add(event);
        }
    });
}

/// Flushes every thread's buffered stream events into the worker channel.
/// Called at shutdown before the worker is signalled to stop.
pub(crate) fn flush_stream_batch() {
    EVENT_REGISTRY.flush_all();
}

impl BatchedMeasurement for StreamEvent {
    type Tx = CbSender<Vec<Self>>;

    fn elapsed_since_start_ns(&self) -> u64 {
        match self {
            StreamEvent::Yielded { timestamp, .. } => crate::channels::timestamp_nanos(*timestamp),
            _ => 0,
        }
    }

    fn fetch_sender() -> Option<Self::Tx> {
        Some(STREAMS_STATE.get()?.event_tx.clone())
    }

    fn send_batch(tx: &Self::Tx, batch: Vec<Self>) {
        let _ = tx.send(batch);
    }

    fn is_flush_boundary(&self) -> bool {
        matches!(self, StreamEvent::Created { .. })
    }
}

fn process_stream_event(state: &mut StreamsInternalState, event: StreamEvent) {
    match event {
        StreamEvent::Created {
            id,
            source,
            display_label,
            type_name,
            type_size,
        } => {
            let iter = state.stats.values().filter(|s| s.source == source).count() as u32;
            state.stats.insert(
                id,
                StreamStats::new(id, source, display_label, type_name, type_size, iter),
            );
            state.logs.insert(id, StreamStatsLogs::new());
        }
        StreamEvent::Yielded { id, log, timestamp } => {
            if let Some(stream_stats) = state.stats.get_mut(&id) {
                stream_stats.items_yielded += 1;
            }
            if let Some(entry_logs) = state.logs.get_mut(&id) {
                let items_yielded = state.stats.get(&id).map_or(0, |s| s.items_yielded);
                let limit = *crate::channels::LOGS_LIMIT;
                if entry_logs.logs.len() >= limit {
                    entry_logs.logs.pop_front();
                }
                entry_logs.logs.push_back(DataFlowLogEntry::new(
                    items_yielded,
                    crate::channels::timestamp_nanos(timestamp),
                    log,
                    None,
                ));
            }
        }
        StreamEvent::Completed { id } => {
            if let Some(stream_stats) = state.stats.get_mut(&id) {
                stream_stats.state = ChannelState::Closed;
            }
        }
    }
}

/// Initialize the stream statistics collection system (called on first instrumented stream).
/// Returns a reference to the global state.
pub(crate) fn init_streams_state() -> &'static StreamsState {
    STREAMS_STATE.get_or_init(|| {
        crate::lib_on::START_TIME.get_or_init(Instant::now);

        let (event_tx, event_rx) = unbounded::<Vec<StreamEvent>>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);
        let inner = Arc::new(RwLock::new(StreamsInternalState {
            stats: HashMap::new(),
            logs: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("hp-meta-streams".into())
            .spawn(move || {
                let mut local_buffer: Vec<StreamEvent> = Vec::with_capacity(WORKER_BATCH_SIZE);
                let flush_interval = std::time::Duration::from_millis(WORKER_FLUSH_INTERVAL_MS);

                loop {
                    select! {
                        recv(event_rx) -> result => {
                            match result {
                                Ok(events) => {
                                    local_buffer.extend(events);
                                    if local_buffer.len() >= WORKER_BATCH_SIZE {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_stream_event(&mut shared, e);
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    if !local_buffer.is_empty() {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_stream_event(&mut shared, e);
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
                                    Ok(events) => drained_events.extend(events),
                                    Err(_) => break,
                                }
                            }

                            if let Ok(mut shared) = inner_clone.write() {
                                for e in local_buffer.drain(..) {
                                    process_stream_event(&mut shared, e);
                                }
                                for event in drained_events {
                                    process_stream_event(&mut shared, event);
                                }
                            }
                            break;
                        }
                        default(flush_interval) => {
                            if !local_buffer.is_empty() {
                                if let Ok(mut shared) = inner_clone.write() {
                                    for e in local_buffer.drain(..) {
                                        process_stream_event(&mut shared, e);
                                    }
                                }
                            }
                        }
                    }
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn stream-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        StreamsState {
            event_tx,
            inner,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
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
/// Optional parameters: `label`, `log = true`.
/// `log = true` requires `Debug` on the item type.
///
/// # Examples
///
/// ```rust,ignore
/// use futures::stream::{self, StreamExt};
/// use hotpath_meta::stream;
///
/// #[tokio::main]
/// async fn main() {
///     let s = stream!(stream::iter(1..=10));
///     let _items: Vec<_> = s.collect().await;
/// }
/// ```
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

/// Compare two stream stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source and iter).
pub(crate) fn compare_stream_stats(a: &StreamStats, b: &StreamStats) -> std::cmp::Ordering {
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
    let Some(state) = STREAMS_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<StreamStats> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_stream_stats);
    stats
}

pub(crate) fn get_streams_json() -> crate::json::JsonStreamsList {
    let data = get_sorted_stream_stats()
        .iter()
        .map(JsonStreamEntry::from)
        .collect();

    crate::json::JsonStreamsList {
        current_elapsed_ns: crate::lib_on::current_elapsed_ns(),
        data,
    }
}

pub(crate) fn get_stream_logs(id: u32) -> Option<StreamLogs> {
    let state = STREAMS_STATE.get()?;
    let guard = state.inner.read().unwrap();
    let entry_logs = guard.logs.get(&id)?;
    let mut yielded_logs: Vec<DataFlowLogEntry> = entry_logs.logs.iter().cloned().collect();
    yielded_logs.sort_by_key(|entry| std::cmp::Reverse(entry.index));
    Some(StreamLogs {
        id,
        logs: yielded_logs,
    })
}
