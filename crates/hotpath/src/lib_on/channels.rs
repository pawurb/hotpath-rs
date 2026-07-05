//! Channel instrumentation module - tracks message flow and channel state.

use crossbeam_channel::{bounded, Receiver as CbReceiver, RecvTimeoutError, Sender as CbSender};
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::instant::Instant;

pub(crate) mod wrapper;

use std::mem;

use crate::batch::{EventProducer, EventQueueRegistry};
use crate::json::JsonChannelEntry;
pub(crate) use crate::json::{ChannelLogs, ChannelState, DataFlowLogEntry};
use crate::lib_on::hotpath_guard::DRAIN_INTERVAL_MS;
use crate::metrics_server::METRICS_SERVER_PORT;

pub use crate::Format;

static CHANNEL_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn next_channel_id() -> u32 {
    CHANNEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Type of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelType {
    Bounded(usize),
    Unbounded,
    Oneshot,
    /// Placeholder for a channel whose `Created` event has not been processed
    /// yet; backfilled with the real type when it arrives.
    Pending,
}

/// Registers a new channel with the profiling subsystem.
///
/// Emits a [`ChannelEvent::Created`] event to the background worker and returns
/// the channel's unique id, which wrappers use to report subsequent
/// send/receive/close events. `T` is the message type carried by the channel
/// and is used to record the type name and per-message byte size.
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
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

static EVENT_QUEUES: EventQueueRegistry<ChannelEvent> = EventQueueRegistry::new();

thread_local! {
    static EVENT_PRODUCER: EventProducer<ChannelEvent> = EVENT_QUEUES.register();
}

#[inline]
pub(crate) fn send_channel_event(event: ChannelEvent) {
    if !EVENT_QUEUES.is_active() {
        return;
    }
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    // `try_with`, not `with`: a `wrap = true` endpoint can emit an event (send,
    // recv, or `Closed` on drop) from a producer thread that is tearing down, when
    // this thread-local may already be destroyed. Dropping the event is fine;
    // panicking in a `Drop` would abort the process.
    let _ = EVENT_PRODUCER.try_with(|producer| producer.push(event));
}

/// Stops producers ahead of the worker's final sweep at shutdown.
pub(crate) fn stop_channel_events() {
    EVENT_QUEUES.set_active(false);
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
    /// Earliest message timestamp (ns since start), shared across both directions.
    /// Anchors the elapsed window used to derive throughput rates.
    first_msg_ns: Option<u64>,
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
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
        // track the minimum rather than the first processed.
        self.first_msg_ns = Some(self.first_msg_ns.map_or(ts_ns, |first| first.min(ts_ns)));
    }

    fn rate_per_sec(&self, count: u64) -> Option<f64> {
        // A oneshot carries a single message, so any rate is meaningless.
        if self.channel_type == ChannelType::Oneshot {
            return None;
        }
        // Below two messages the distribution is too sparse to report a rate.
        if count < 2 {
            return None;
        }
        // Anchor throughput to elapsed observation time since the first message
        // rather than the first-to-last message span. The span collapses to a
        // single inter-event gap for sparse channels and yields absurd rates;
        // dividing by real elapsed time stays bounded and cannot blow up.
        let first = self.first_msg_ns?;
        let elapsed_ns = crate::lib_on::current_elapsed_ns().checked_sub(first)?;
        if elapsed_ns == 0 {
            return None;
        }
        Some(count as f64 / (elapsed_ns as f64 / 1e9))
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
    pub(crate) inner: Arc<RwLock<ChannelsInternalState>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<()>>>,
}

pub(crate) static CHANNELS_STATE: OnceLock<ChannelsState> = OnceLock::new();

pub(crate) use crate::lib_on::START_TIME;

pub(crate) use crate::lib_on::hotpath_guard::LOGS_LIMIT;

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
/// Entry for events that arrive ahead of their `Created` (sweeps only preserve
/// per-thread order, so another thread's data events can be drained first).
/// `Created` backfills the metadata; `wrap` is inferred from the event variant
/// so wrap-only stats (queue depth, processing histogram) record immediately.
fn placeholder_channel_entry(id: u32, wrap: bool) -> ChannelEntry {
    ChannelEntry::new(id, "", None, ChannelType::Pending, "", 0, wrap, 0)
}

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
            let entry = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, wrap));
            entry.source = source;
            entry.label = display_label;
            entry.channel_type = channel_type;
            entry.type_name = type_name;
            entry.type_size = type_size;
            entry.wrap = wrap;
            entry.iter = iter;
            if wrap && entry.proc_hist.is_none() {
                entry.proc_hist = Some(ChannelEntry::new_histogram());
            }
            state.logs.entry(id).or_insert_with(ChannelEntryLogs::new);
        }
        ChannelEvent::MessageSent { id, log, timestamp } => {
            let ts_ns = timestamp_nanos(timestamp);
            let channel_stats = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, false));
            channel_stats.sent_count += 1;
            channel_stats.record_activity(ts_ns);
            channel_stats.update_state();
            let sent_count = channel_stats.sent_count;

            let entry_logs = state.logs.entry(id).or_insert_with(ChannelEntryLogs::new);
            let limit = *LOGS_LIMIT;
            if entry_logs.sent_logs.len() >= limit {
                entry_logs.sent_logs.pop_front();
            }
            entry_logs
                .sent_logs
                .push_back(DataFlowLogEntry::new(sent_count, ts_ns, log, None, None));
        }
        ChannelEvent::MessageReceived { id, timestamp } => {
            let ts_ns = timestamp_nanos(timestamp);
            let channel_stats = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, false));
            channel_stats.received_count += 1;
            channel_stats.record_activity(ts_ns);
            channel_stats.update_state();
            let received_count = channel_stats.received_count;

            let entry_logs = state.logs.entry(id).or_insert_with(ChannelEntryLogs::new);
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
        ChannelEvent::WrapMessageSent {
            id,
            msg_id,
            log,
            timestamp,
            queue_len,
        } => {
            let ts_ns = timestamp_nanos(timestamp);
            let channel_stats = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, true));
            channel_stats.sent_count += 1;
            channel_stats.record_activity(ts_ns);
            channel_stats.update_state();
            channel_stats.record_queue(queue_len);
            let sent_count = channel_stats.sent_count;

            let entry_logs = state.logs.entry(id).or_insert_with(ChannelEntryLogs::new);
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
        ChannelEvent::WrapMessageReceived {
            id,
            msg_id,
            timestamp,
            queue_len,
            delay_nanos,
        } => {
            let ts_ns = timestamp_nanos(timestamp);
            let channel_stats = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, true));
            channel_stats.received_count += 1;
            channel_stats.record_activity(ts_ns);
            channel_stats.update_state();
            channel_stats.record_queue(queue_len);
            channel_stats.record_proc(delay_nanos);
            let received_count = channel_stats.received_count;

            let entry_logs = state.logs.entry(id).or_insert_with(ChannelEntryLogs::new);
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
        ChannelEvent::Closed { id } => {
            state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, false))
                .state = ChannelState::Closed;
        }
        ChannelEvent::Notified { id } => {
            let channel_stats = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_channel_entry(id, false));
            if channel_stats.state != ChannelState::Closed {
                channel_stats.state = ChannelState::Notified;
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
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
pub(crate) fn init_channels_state() -> &'static ChannelsState {
    CHANNELS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);
        let inner = Arc::new(RwLock::new(ChannelsInternalState {
            stats: HashMap::new(),
            logs: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        EVENT_QUEUES.set_active(true);

        std::thread::Builder::new()
            .name("hp-channels".into())
            .spawn(move || {
                let flush_interval = std::time::Duration::from_millis(*DRAIN_INTERVAL_MS);
                let mut swept: Vec<ChannelEvent> = Vec::new();

                // Single consumer of the per-thread event queues: capped sweep on
                // every tick, then one uncapped drain when shutdown is signalled
                // (producers are already stopped by then, so nothing is left behind).
                loop {
                    let shutdown = !matches!(
                        shutdown_rx.recv_timeout(flush_interval),
                        Err(RecvTimeoutError::Timeout)
                    );

                    if shutdown {
                        EVENT_QUEUES.drain_all(&mut swept);
                        flush_channel_buffer(&mut swept, &inner_clone);
                        break;
                    }

                    EVENT_QUEUES.sweep(&mut swept);
                    flush_channel_buffer(&mut swept, &inner_clone);
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn channel-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        ChannelsState {
            inner,
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
pub trait InstrumentChannelProxy {
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
pub trait InstrumentChannelProxyLog {
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
/// Returns wrapper types (`hotpath::wrap::<backend>::{Sender, Receiver}`) instead of
/// the original channel types, so queue depth is measured exactly with no forwarder.
/// This is the default mode of the `channel!` macro; not intended for direct use.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "channel type `{Self}` cannot be instrumented by the default `channel!` mode",
    note = "this backend is forwarder-only; pass `proxy = true`, e.g. `channel!(expr, proxy = true)`"
)]
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
/// `log = true`.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "channel type `{Self}` cannot be instrumented by the default `channel!` mode",
    note = "this backend is forwarder-only; pass `proxy = true`, e.g. `channel!(expr, proxy = true, log = true)`"
)]
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

/// Instrument a channel creation for profiling.
///
/// By default the macro **wraps the endpoints** (`wrap` mode): it returns
/// `hotpath::wrap::<backend>::{Sender, Receiver}` instead of the raw channel types and
/// measures exact queue depth plus send->receive latency, with no forwarder task.
///
/// Optional parameters: `label`, `log = true`, `capacity`, `proxy = true` (in any order).
/// `log = true` requires `Debug` on the message type.
///
/// # Default (wrap) mode
///
/// The channel expression **must be constructed inline**, e.g.
/// `channel!(crossbeam_channel::unbounded::<T>())`. The wrapper rebuilds the inner channel
/// (to carry a per-message id) and discards the one you pass in, so any endpoint cloned
/// before wrapping is orphaned and its messages are silently dropped. Clone the returned
/// wrapper endpoints instead.
///
/// Bounded `std::sync::mpsc` (`sync_channel`) cannot recover its capacity from the
/// endpoint, so `capacity = N` is required, e.g.
/// `channel!(std::sync::mpsc::sync_channel::<T>(100), capacity = 100)`. **The value must
/// match the `sync_channel(N)` argument** - wrap mode rebuilds the inner channel from
/// `capacity`, so a mismatch silently changes backpressure (and only in profiled builds,
/// since with `hotpath` off `channel!` returns your original channel untouched). std
/// exposes no capacity accessor, so keep the two numbers equal. Unbounded std, crossbeam,
/// flume, tokio and async-channel wrappers recover the bound from the endpoint and need no
/// `capacity`.
///
/// # `proxy = true` (forwarder mode)
///
/// Passing `proxy = true` selects the forwarder-based mode: the original endpoint types are
/// preserved (type-transparent) and a background task/thread relays every message through a
/// second channel. This is the only mode available for backends without a wrap
/// implementation (`futures_channel`, `tokio::sync::oneshot`); using them without
/// `proxy = true` is a compile error that points you here. `capacity` is required for
/// `futures_channel::mpsc` bounded channels.
///
/// # Examples
///
/// ```rust,no_run
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///    let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100));
///
///    tx.send("Hello".to_string()).await.unwrap();
/// }
/// ```
#[macro_export]
macro_rules! channel {
    // Default: wrap mode. `channel!(expr)` -> endpoint-wrapping instrumentation.
    ($expr:expr) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentChannelWrap::instrument_wrap($expr, CHANNEL_ID, None, None)
    }};

    // Any argument list is parsed order-independently by the muncher below. Slots are
    // `label capacity log proxy`; `label`/`capacity` are stored as ready-to-use `Option`
    // tokens so the dispatch only branches on `log` x `proxy`. `CHANNEL_ID` is captured
    // once here so `file!()`/`line!()` resolve to the user's call site.
    ($expr:expr, $($rest:tt)*) => {{
        const CHANNEL_ID: &'static str = concat!(file!(), ":", line!());
        $crate::channel!(@munch CHANNEL_ID, $expr ; (None) (None) [nolog] [wrap] ; $($rest)*)
    }};

    (@munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $proxy:tt ;) => {
        $crate::channel!(@dispatch $id, $e ; $lbl $cap $log $proxy)
    };
    (@munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $proxy:tt ; proxy = true $(, $($r:tt)*)?) => {
        $crate::channel!(@munch $id, $e ; $lbl $cap $log [proxy] ; $($($r)*)?)
    };
    (@munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $proxy:tt ; label = $l:expr $(, $($r:tt)*)?) => {
        $crate::channel!(@munch $id, $e ; (Some($l.to_string())) $cap $log $proxy ; $($($r)*)?)
    };
    (@munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $proxy:tt ; capacity = $c:expr $(, $($r:tt)*)?) => {
        $crate::channel!(@munch $id, $e ; $lbl (Some($c)) $log $proxy ; $($($r)*)?)
    };
    (@munch $id:ident, $e:expr ; $lbl:tt $cap:tt $log:tt $proxy:tt ; log = true $(, $($r:tt)*)?) => {
        $crate::channel!(@munch $id, $e ; $lbl $cap [log] $proxy ; $($($r)*)?)
    };

    (@dispatch $id:ident, $e:expr ; $lbl:tt $cap:tt [nolog] [wrap]) => {
        $crate::InstrumentChannelWrap::instrument_wrap($e, $id, $lbl, $cap)
    };
    (@dispatch $id:ident, $e:expr ; $lbl:tt $cap:tt [log] [wrap]) => {
        $crate::InstrumentChannelWrapLog::instrument_wrap_log($e, $id, $lbl, $cap)
    };
    (@dispatch $id:ident, $e:expr ; $lbl:tt $cap:tt [nolog] [proxy]) => {
        $crate::InstrumentChannelProxy::instrument($e, $id, $lbl, $cap)
    };
    (@dispatch $id:ident, $e:expr ; $lbl:tt $cap:tt [log] [proxy]) => {
        $crate::InstrumentChannelProxyLog::instrument_log($e, $id, $lbl, $cap)
    };
}

/// Compare two channel stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source and iter).
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
    let Some(state) = CHANNELS_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<ChannelEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_channel_entries);
    stats
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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
