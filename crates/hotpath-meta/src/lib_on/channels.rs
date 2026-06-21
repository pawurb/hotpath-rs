//! Channel instrumentation module - tracks message flow and channel state.

use crossbeam_channel::{
    bounded, unbounded, Receiver as CbReceiver, Select, Sender as CbSender, TryRecvError,
};
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::instant::Instant;

pub(crate) mod wrapper;

use std::mem;

use crate::batch::{register_thread_batch, BatchRegistry, BatchedMeasurement, MeasurementBatch};
use crate::json::JsonChannelEntry;
pub(crate) use crate::json::{ChannelLogs, ChannelState, DataFlowLogEntry};
use crate::lib_on::hotpath_guard::{
    WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
use crate::metrics_server::METRICS_SERVER_PORT;

pub use crate::Format;

static CHANNEL_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub(crate) fn next_channel_id() -> u32 {
    CHANNEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
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
/// Emits a [`ChannelEvent::Created`] event to the background worker and returns
/// the channel's unique id, which wrappers use to report subsequent
/// send/receive/close events. `T` is the message type carried by the channel
/// and is used to record the type name and per-message byte size.
pub(crate) fn register_channel<T>(
    source: &'static str,
    label: Option<String>,
    channel_type: ChannelType,
) -> u32 {
    register_channel_inner::<T>(source, label, channel_type, false)
}

/// Like [`register_channel`] but marks the channel as endpoint-wrapped
/// (`wrap = true`). Used by the instrumented endpoint wrappers in
/// `wrapper/*_wrap.rs`.
#[cfg_attr(not(feature = "crossbeam"), allow(dead_code))]
pub(crate) fn register_channel_wrap<T>(
    source: &'static str,
    label: Option<String>,
    channel_type: ChannelType,
) -> u32 {
    register_channel_inner::<T>(source, label, channel_type, true)
}

fn register_channel_inner<T>(
    source: &'static str,
    label: Option<String>,
    channel_type: ChannelType,
    wrap: bool,
) -> u32 {
    let type_name = std::any::type_name::<T>();
    init_channels_state();
    let id = next_channel_id();

    send_channel_event(ChannelEvent::Created {
        id,
        source,
        display_label: label,
        channel_type,
        type_name,
        type_size: mem::size_of::<T>(),
        wrap,
    });

    id
}

static EVENT_REGISTRY: BatchRegistry<ChannelEvent> = BatchRegistry::new();

thread_local! {
    static EVENT_BATCH: std::sync::Arc<std::sync::Mutex<MeasurementBatch<ChannelEvent>>> =
        register_thread_batch(&EVENT_REGISTRY);
}

#[inline]
pub(crate) fn send_channel_event(event: ChannelEvent) {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    // `try_with`, not `with`: a `wrap = true` endpoint can emit an event (send,
    // recv, or `Closed` on drop) from a producer thread that is tearing down, when
    // this thread-local may already be destroyed. Dropping the event is fine;
    // panicking in a `Drop` would abort the process.
    let _ = EVENT_BATCH.try_with(|b| {
        if let Ok(mut b) = b.lock() {
            b.add(event);
        }
    });
}

/// Flushes every thread's buffered channel events into the worker channel.
/// Called at shutdown before the worker is signalled to stop.
pub(crate) fn flush_channel_batch() {
    EVENT_REGISTRY.flush_all();
}

impl BatchedMeasurement for ChannelEvent {
    type Tx = CbSender<Vec<Self>>;

    fn elapsed_since_start_ns(&self) -> u64 {
        match self {
            ChannelEvent::MessageSent { timestamp, .. }
            | ChannelEvent::MessageReceived { timestamp, .. }
            | ChannelEvent::WrapMessageSent { timestamp, .. }
            | ChannelEvent::WrapMessageReceived { timestamp, .. } => timestamp_nanos(*timestamp),
            _ => 0,
        }
    }

    fn fetch_sender() -> Option<Self::Tx> {
        Some(CHANNELS_STATE.get()?.event_tx.clone())
    }

    fn send_batch(tx: &Self::Tx, batch: Vec<Self>) {
        let _ = tx.send(batch);
    }

    fn is_flush_boundary(&self) -> bool {
        matches!(self, ChannelEvent::Created { .. })
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
    /// Earliest and latest message timestamps (ns since start), shared across both
    /// directions. Define the active window used to derive throughput rates.
    first_msg_ns: Option<u64>,
    last_msg_ns: Option<u64>,
    pub(crate) type_name: &'static str,
    pub(crate) type_size: usize,
    pub(crate) wrap: bool,
    /// Exact channel depth, only tracked for `wrap` channels. `None` for proxy channels.
    /// Derived from `sent_count - received_count` (converged value order-independent).
    pub(crate) queue_size: Option<usize>,
    pub(crate) max_queue_size: Option<usize>,
    /// Avg denominator is `received_count` (one delay recorded per receive).
    pub(crate) proc_total_nanos: u64,
    /// `Some` only for `wrap` channels; `None` for proxy channels, which cannot
    /// measure latency accurately.
    proc_hist: Option<Histogram<u64>>,
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

pub(crate) fn channel_to_json(stats: &ChannelEntry, percentiles: &[f64]) -> JsonChannelEntry {
    let label = resolve_label(stats.source, stats.label.as_deref(), Some(stats.iter));

    let mut proc_percentiles = HashMap::new();
    let proc_avg = if stats.has_proc_hist() {
        for &p in percentiles {
            proc_percentiles.insert(
                crate::output::format_percentile_key(p),
                crate::output::format_duration(stats.proc_percentile_nanos(p)),
            );
        }
        Some(crate::output::format_duration(stats.proc_avg_nanos()))
    } else {
        None
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
        sent_per_sec: stats.sent_per_sec(),
        received_per_sec: stats.received_per_sec(),
        type_name: stats.type_name.to_string(),
        type_size: stats.type_size,
        wrap: stats.wrap,
        queue_size: stats.queue_size,
        max_queue_size: stats.max_queue_size,
        proc_avg,
        proc_percentiles,
        iter: stats.iter,
    }
}

impl ChannelEntry {
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: u32,
        source: &'static str,
        label: Option<String>,
        channel_type: ChannelType,
        type_name: &'static str,
        type_size: usize,
        wrap: bool,
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
            first_msg_ns: None,
            last_msg_ns: None,
            type_name,
            type_size,
            wrap,
            queue_size: None,
            max_queue_size: None,
            proc_total_nanos: 0,
            proc_hist: wrap.then(Self::new_histogram),
            iter,
        }
    }

    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    fn new_histogram() -> Histogram<u64> {
        Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
            .expect("hdrhistogram init")
    }

    #[inline]
    fn record_proc(&mut self, nanos: u64) {
        if let Some(ref mut hist) = self.proc_hist {
            self.proc_total_nanos += nanos;
            hist.record(nanos.clamp(Self::LOW_NS, Self::HIGH_NS))
                .unwrap();
        }
    }

    pub(crate) fn has_proc_hist(&self) -> bool {
        self.proc_hist.is_some()
    }

    #[inline]
    fn record_activity(&mut self, ts_ns: u64) {
        // Per-thread batch flushing can deliver events out of timestamp order, so
        // track the min/max extrema rather than first/last processed.
        self.first_msg_ns = Some(self.first_msg_ns.map_or(ts_ns, |first| first.min(ts_ns)));
        self.last_msg_ns = Some(self.last_msg_ns.map_or(ts_ns, |last| last.max(ts_ns)));
    }

    fn rate_per_sec(&self, count: u64) -> Option<f64> {
        // A oneshot carries a single message, so its active window is just the
        // send-to-receive gap; dividing by that microsecond-scale span yields a
        // meaningless rate.
        if self.channel_type == ChannelType::Oneshot {
            return None;
        }
        let (first, last) = (self.first_msg_ns?, self.last_msg_ns?);
        if last <= first {
            return None;
        }
        Some(count as f64 / ((last - first) as f64 / 1e9))
    }

    pub(crate) fn sent_per_sec(&self) -> Option<f64> {
        self.rate_per_sec(self.sent_count)
    }

    pub(crate) fn received_per_sec(&self) -> Option<f64> {
        self.rate_per_sec(self.received_count)
    }

    pub(crate) fn proc_avg_nanos(&self) -> u64 {
        self.proc_total_nanos
            .checked_div(self.received_count)
            .unwrap_or(0)
    }

    pub(crate) fn proc_percentile_nanos(&self, p: f64) -> u64 {
        match &self.proc_hist {
            Some(hist) if self.received_count > 0 => hist.value_at_percentile(p.clamp(0.0, 100.0)),
            _ => 0,
        }
    }

    /// Peak comes only from real `len()` snapshots; max of those is order-independent,
    /// so it stays a true high-water mark. Current depth is counts-derived
    /// (`sent - received`), exact once the channel is idle since the counters commute,
    /// but it can transiently overshoot when a producer batch reaches the worker ahead
    /// of the matching consumer batch - clamping to `max` keeps `current <= max`.
    fn record_queue(&mut self, queue_len: usize) {
        let max = self.max_queue_size.unwrap_or(0).max(queue_len);
        self.max_queue_size = Some(max);
        let depth = self.sent_count.saturating_sub(self.received_count) as usize;
        self.queue_size = Some(depth.min(max));
    }

    fn update_state(&mut self) {
        if self.state == ChannelState::Closed || self.state == ChannelState::Notified {
            return;
        }
        self.state = ChannelState::Active;
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
        wrap: bool,
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
    WrapMessageSent {
        id: u32,
        msg_id: u64,
        log: Option<String>,
        timestamp: Instant,
        queue_len: usize,
    },
    WrapMessageReceived {
        id: u32,
        msg_id: u64,
        timestamp: Instant,
        queue_len: usize,
        delay_nanos: u64,
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
    pub(crate) event_tx: CbSender<Vec<ChannelEvent>>,
    pub(crate) inner: Arc<RwLock<ChannelsInternalState>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<()>>>,
}

pub(crate) static CHANNELS_STATE: OnceLock<ChannelsState> = OnceLock::new();

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
            wrap,
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
                    wrap,
                    iter,
                ),
            );
            state.logs.insert(id, ChannelEntryLogs::new());
        }
        ChannelEvent::MessageSent { id, log, timestamp } => {
            let ts_ns = timestamp_nanos(timestamp);
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.sent_count += 1;
                channel_stats.record_activity(ts_ns);
                channel_stats.update_state();
            }
            if let Some(entry_logs) = state.logs.get_mut(&id) {
                let sent_count = state.stats.get(&id).map_or(0, |s| s.sent_count);
                let limit = *LOGS_LIMIT;
                if entry_logs.sent_logs.len() >= limit {
                    entry_logs.sent_logs.pop_front();
                }
                entry_logs
                    .sent_logs
                    .push_back(DataFlowLogEntry::new(sent_count, ts_ns, log, None, None));
            }
        }
        ChannelEvent::MessageReceived { id, timestamp } => {
            let ts_ns = timestamp_nanos(timestamp);
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.received_count += 1;
                channel_stats.record_activity(ts_ns);
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
                    ts_ns,
                    None,
                    None,
                    None,
                ));
            }
        }
        ChannelEvent::WrapMessageSent {
            id,
            msg_id,
            log,
            timestamp,
            queue_len,
        } => {
            let ts_ns = timestamp_nanos(timestamp);
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.sent_count += 1;
                channel_stats.record_activity(ts_ns);
                channel_stats.update_state();
                channel_stats.record_queue(queue_len);
            }
            if let Some(entry_logs) = state.logs.get_mut(&id) {
                let sent_count = state.stats.get(&id).map_or(0, |s| s.sent_count);
                let limit = *LOGS_LIMIT;
                if entry_logs.sent_logs.len() >= limit {
                    entry_logs.sent_logs.pop_front();
                }
                entry_logs.sent_logs.push_back(DataFlowLogEntry::new(
                    sent_count,
                    ts_ns,
                    log,
                    None,
                    Some(msg_id),
                ));
            }
        }
        ChannelEvent::WrapMessageReceived {
            id,
            msg_id,
            timestamp,
            queue_len,
            delay_nanos,
        } => {
            let ts_ns = timestamp_nanos(timestamp);
            if let Some(channel_stats) = state.stats.get_mut(&id) {
                channel_stats.received_count += 1;
                channel_stats.record_activity(ts_ns);
                channel_stats.update_state();
                channel_stats.record_queue(queue_len);
                channel_stats.record_proc(delay_nanos);
            }
            if let Some(entry_logs) = state.logs.get_mut(&id) {
                let received_count = state.stats.get(&id).map_or(0, |s| s.received_count);
                let limit = *LOGS_LIMIT;
                if entry_logs.received_logs.len() >= limit {
                    entry_logs.received_logs.pop_front();
                }
                entry_logs.received_logs.push_back(DataFlowLogEntry::new(
                    received_count,
                    ts_ns,
                    None,
                    None,
                    Some(msg_id),
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
                if channel_stats.state != ChannelState::Closed {
                    channel_stats.state = ChannelState::Notified;
                }
            }
        }
    }
}

fn flush_channel_buffer(
    buffer: &mut Vec<ChannelEvent>,
    inner: &Arc<RwLock<ChannelsInternalState>>,
) {
    if buffer.is_empty() {
        return;
    }
    if let Ok(mut shared) = inner.write() {
        for e in buffer.drain(..) {
            process_channel_event(&mut shared, e);
        }
    }
}

/// Initialize the channel statistics collection system (called on first instrumented channel).
pub(crate) fn init_channels_state() -> &'static ChannelsState {
    CHANNELS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (event_tx, event_rx) = unbounded::<Vec<ChannelEvent>>();
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

                // Shutdown is checked before events; the `ready_timeout` tick flushes a partial buffer.
                let mut select = Select::new();
                let _shutdown_idx = select.recv(&shutdown_rx);
                let _event_idx = select.recv(&event_rx);

                loop {
                    if select.ready_timeout(flush_interval).is_err() {
                        flush_channel_buffer(&mut local_buffer, &inner_clone);
                        continue;
                    }

                    if !matches!(shutdown_rx.try_recv(), Err(TryRecvError::Empty)) {
                        for _ in 0..WORKER_SHUTDOWN_DRAIN_LIMIT {
                            match event_rx.try_recv() {
                                Ok(events) => local_buffer.extend(events),
                                Err(_) => break,
                            }
                        }
                        flush_channel_buffer(&mut local_buffer, &inner_clone);
                        break;
                    }

                    match event_rx.try_recv() {
                        Ok(events) => {
                            local_buffer.extend(events);
                            if local_buffer.len() >= WORKER_BATCH_SIZE {
                                flush_channel_buffer(&mut local_buffer, &inner_clone);
                            }
                        }
                        // A disconnected receiver stays ready; flush and stop, do not spin.
                        Err(TryRecvError::Disconnected) => {
                            flush_channel_buffer(&mut local_buffer, &inner_clone);
                            break;
                        }
                        Err(TryRecvError::Empty) => {}
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
    let mut parts = path.rsplitn(3, '/');
    match (parts.next(), parts.next()) {
        (Some(last), Some(second_last)) => format!("{}/{}", second_last, last),
        _ => path.to_string(),
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

/// Trait for instrumenting channels by wrapping their endpoints directly.
///
/// Returns wrapper types (`hotpath_meta::wrap::<backend>::{Sender, Receiver}`) instead of
/// the original channel types, so queue depth is measured exactly with no forwarder.
/// Not intended for direct use. Use the `channel!` macro with `wrap = true` instead.
#[doc(hidden)]
pub trait InstrumentChannelWrap {
    type Output;
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output;
}

/// Trait for instrumenting channels by wrapping their endpoints, with message logging.
///
/// This trait is not intended for direct use. Use the `channel!` macro with
/// `wrap = true, log = true` instead.
#[doc(hidden)]
pub trait InstrumentChannelWrapLog {
    type Output;
    fn instrument_wrap_log(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output;
}

cfg_if::cfg_if! {
    if #[cfg(any(feature = "tokio", feature = "futures", feature = "async-channel", feature = "flume"))] {
        pub(crate) static RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .build()
                .unwrap()
        });
    }
}

/// Instrument a channel creation to wrap it with debugging proxies.
///
/// Optional parameters: `label`, `log = true`, `capacity` (in any order).
/// `capacity` is required for `futures_channel::mpsc` bounded channels.
/// `log = true` requires `Debug` on the message type.
///
/// # `wrap = true`
///
/// The channel expression **must be constructed inline**, e.g.
/// `channel!(crossbeam_channel::unbounded::<T>(), wrap = true)`. The wrapper rebuilds
/// the inner channel (to carry a per-message id) and discards the one you pass in, so
/// any endpoint cloned before wrapping is orphaned and its messages are silently
/// dropped. Clone the returned wrapper endpoints instead.
///
/// Bounded `std::sync::mpsc` wrappers (`sync_channel`) cannot recover their capacity
/// from the endpoint, so `capacity = N` is required, e.g.
/// `channel!(std::sync::mpsc::sync_channel::<T>(100), wrap = true, capacity = 100)`.
/// Unbounded std and crossbeam wrappers need no `capacity`.
///
/// **The `capacity` you pass must match the `sync_channel(N)` argument.** Wrap mode
/// rebuilds the inner channel from `capacity` and discards the one you constructed, so a
/// mismatch (e.g. `sync_channel(100)` with `capacity = 1`) silently builds a different
/// bounded channel - and only in profiled builds: with `hotpath-meta` off, `channel!`
/// returns your original `sync_channel(100)` untouched. The result is different
/// backpressure (and potentially a deadlock) that appears only when profiling. There is
/// no way to verify this for you, because std exposes no capacity accessor - keep the two
/// numbers equal.
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
    // Wrap mode (`wrap = true`) returns instrumented endpoint wrappers
    // (`hotpath_meta::wrap::<backend>::{Sender, Receiver}`) for exact queue tracking.
    // `wrap`, `label`, and `log` may appear in any order.
    ($expr:expr, wrap = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrap::instrument_wrap($expr, CHANNEL_ID, None, None)
    }};

    ($expr:expr, wrap = true, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrap::instrument_wrap(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, wrap = true, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($expr, CHANNEL_ID, None, None)
    }};

    ($expr:expr, wrap = true, label = $label:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, wrap = true, log = true, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    // Wrap mode with explicit `capacity` (required for bounded `std::sync::mpsc`
    // wrappers, which cannot recover their capacity from the endpoint).
    ($expr:expr, wrap = true, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrap::instrument_wrap($expr, CHANNEL_ID, None, Some($capacity))
    }};

    ($expr:expr, capacity = $capacity:expr, wrap = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrap::instrument_wrap($expr, CHANNEL_ID, None, Some($capacity))
    }};

    ($expr:expr, wrap = true, capacity = $capacity:expr, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrap::instrument_wrap(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, wrap = true, label = $label:expr, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrap::instrument_wrap(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, wrap = true, capacity = $capacity:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            None,
            Some($capacity),
        )
    }};

    ($expr:expr, wrap = true, log = true, capacity = $capacity:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            None,
            Some($capacity),
        )
    }};

    ($expr:expr, wrap = true, capacity = $capacity:expr, label = $label:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, wrap = true, label = $label:expr, capacity = $capacity:expr, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        const _: usize = $capacity;
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            Some($capacity),
        )
    }};

    ($expr:expr, label = $label:expr, wrap = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrap::instrument_wrap(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, log = true, wrap = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($expr, CHANNEL_ID, None, None)
    }};

    ($expr:expr, label = $label:expr, wrap = true, log = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, log = true, wrap = true, label = $label:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, label = $label:expr, log = true, wrap = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

    ($expr:expr, log = true, label = $label:expr, wrap = true) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrapLog::instrument_wrap_log(
            $expr,
            CHANNEL_ID,
            Some($label.to_string()),
            None,
        )
    }};

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

    // Order-independent muncher for wrap-mode arguments. Accumulates `label`, `capacity`
    // and `log` in any order (the explicit arms above cover the common no-capacity
    // orders; this handles the rest, notably bounded-std `capacity` permutations).
    (@wrap_munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $wrap:tt ;) => {
        $crate::channel!(@wrap_dispatch $id, $e ; $lbl $cap $log $wrap)
    };
    (@wrap_munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $wrap:tt ; wrap = true $(, $($r:tt)*)?) => {
        $crate::channel!(@wrap_munch $id, $e ; $lbl $cap $log [wrap] ; $($($r)*)?)
    };
    (@wrap_munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $wrap:tt ; label = $l:expr $(, $($r:tt)*)?) => {
        $crate::channel!(@wrap_munch $id, $e ; [$l] $cap $log $wrap ; $($($r)*)?)
    };
    (@wrap_munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $wrap:tt ; capacity = $c:expr $(, $($r:tt)*)?) => {
        $crate::channel!(@wrap_munch $id, $e ; $lbl [$c] $log $wrap ; $($($r)*)?)
    };
    (@wrap_munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $wrap:tt ; log = true $(, $($r:tt)*)?) => {
        $crate::channel!(@wrap_munch $id, $e ; $lbl $cap [log] $wrap ; $($($r)*)?)
    };

    (@wrap_dispatch $id:ident, $e:expr ; [$l:expr] [$c:expr] [log] [wrap]) => {
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($e, $id, Some($l.to_string()), Some($c))
    };
    (@wrap_dispatch $id:ident, $e:expr ; [$l:expr] [$c:expr] [nolog] [wrap]) => {
        $crate::InstrumentChannelWrap::instrument_wrap($e, $id, Some($l.to_string()), Some($c))
    };
    (@wrap_dispatch $id:ident, $e:expr ; [] [$c:expr] [log] [wrap]) => {
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($e, $id, None, Some($c))
    };
    (@wrap_dispatch $id:ident, $e:expr ; [] [$c:expr] [nolog] [wrap]) => {
        $crate::InstrumentChannelWrap::instrument_wrap($e, $id, None, Some($c))
    };
    (@wrap_dispatch $id:ident, $e:expr ; [$l:expr] [] [log] [wrap]) => {
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($e, $id, Some($l.to_string()), None)
    };
    (@wrap_dispatch $id:ident, $e:expr ; [$l:expr] [] [nolog] [wrap]) => {
        $crate::InstrumentChannelWrap::instrument_wrap($e, $id, Some($l.to_string()), None)
    };
    (@wrap_dispatch $id:ident, $e:expr ; [] [] [log] [wrap]) => {
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($e, $id, None, None)
    };
    (@wrap_dispatch $id:ident, $e:expr ; [] [] [nolog] [wrap]) => {
        $crate::InstrumentChannelWrap::instrument_wrap($e, $id, None, None)
    };
    (@wrap_dispatch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt [nowrap]) => {
        compile_error!("channel!: unsupported argument combination")
    };

    // Fallback entry for `wrap = true` calls whose argument order is not covered by an
    // explicit arm above. `CHANNEL_ID` is captured once here at the call site and
    // threaded through the muncher so `file!()`/`line!()` resolve to the user's location.
    ($expr:expr, $($rest:tt)*) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::channel!(@wrap_munch CHANNEL_ID, $expr ; [] [] [nolog] [nowrap] ; $($rest)*)
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

pub(crate) fn get_channels_json() -> crate::json::JsonChannelsList {
    let percentiles = crate::lib_on::hotpath_guard::configured_percentiles();
    let data = get_sorted_channel_entries()
        .iter()
        .map(|entry| channel_to_json(entry, &percentiles))
        .collect();

    crate::json::JsonChannelsList {
        current_elapsed_ns: crate::lib_on::current_elapsed_ns(),
        percentiles,
        data,
    }
}

pub(crate) fn get_channel_logs(id: u32) -> Option<ChannelLogs> {
    let state = CHANNELS_STATE.get()?;
    let guard = state.inner.read().unwrap();
    let entry_logs = guard.logs.get(&id)?;
    Some(ChannelLogs {
        id,
        sent_logs: entry_logs.sent_logs.iter().rev().cloned().collect(),
        received_logs: entry_logs.received_logs.iter().rev().cloned().collect(),
    })
}

#[cfg(test)]
mod tests {
    use crate::channels::{
        process_channel_event, ChannelEvent, ChannelType, ChannelsInternalState,
    };
    use crate::instant::Instant;
    use std::collections::HashMap;

    /// Current depth is counts-derived, so it converges exactly regardless of arrival
    /// order even when per-thread batches reach the worker out of order with equal
    /// `Instant` timestamps. Peak tracks the real `len()` snapshot and current is
    /// clamped to it, so `current <= max`.
    #[test]
    fn out_of_order_queue_snapshot_converges_within_peak() {
        let mut state = ChannelsInternalState {
            stats: HashMap::new(),
            logs: HashMap::new(),
        };

        let id = 1;
        process_channel_event(
            &mut state,
            ChannelEvent::Created {
                id,
                source: "test",
                display_label: None,
                channel_type: ChannelType::Unbounded,
                type_name: "u8",
                type_size: 1,
                wrap: true,
            },
        );

        // Same tick for both ops: the equal-timestamp case the old `>=` tiebreak could
        // not resolve. The receive batch arrives first, then the send batch.
        let ts = Instant::now();
        process_channel_event(
            &mut state,
            ChannelEvent::WrapMessageReceived {
                id,
                msg_id: 1,
                timestamp: ts,
                queue_len: 0,
                delay_nanos: 0,
            },
        );
        process_channel_event(
            &mut state,
            ChannelEvent::WrapMessageSent {
                id,
                msg_id: 1,
                log: None,
                timestamp: ts,
                queue_len: 1,
            },
        );

        let entry = state.stats.get(&id).expect("channel registered");

        // One sent, one received → drained. Depth is counts-derived, so arrival order
        // (and the equal timestamps) cannot make a stale snapshot win.
        assert_eq!(
            entry.queue_size,
            Some(0),
            "current depth must equal sent_count - received_count regardless of order"
        );

        // Peak is the real `len()` snapshot (1); current is clamped to it.
        assert_eq!(entry.max_queue_size, Some(1));
        assert!(entry.queue_size <= entry.max_queue_size);
    }

    #[test]
    fn closed_channel_state_is_terminal() {
        let mut entry = crate::channels::ChannelEntry::new(
            1,
            "test",
            None,
            crate::channels::ChannelType::Bounded(1),
            "u8",
            1,
            false,
            0,
        );
        entry.state = crate::channels::ChannelState::Closed;

        entry.update_state();

        assert_eq!(entry.state, crate::channels::ChannelState::Closed);
    }
}
