//! Byte-level I/O instrumentation - tracks read/write/flush/shutdown operations
//! performed through wrapped `Read`/`Write`/`AsyncRead`/`AsyncWrite` values.
//!
//! Wrapping the underlying resource (file, socket) measures actual resource
//! I/O; wrapping a `BufReader`/`BufWriter` measures application-facing
//! buffered operations instead.

use crossbeam_channel::{bounded, Receiver as CbReceiver, RecvTimeoutError, Sender as CbSender};
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, RwLock as StdRwLock};

use crate::batch::{EventProducer, EventQueueRegistry};
use crate::instant::Instant;
use crate::lib_on::hotpath_guard::DRAIN_INTERVAL_MS;
use crate::lib_on::START_TIME;
use crate::metrics_server::METRICS_SERVER_PORT;

pub(crate) mod wrapper;

pub use wrapper::InstrumentedIo;

static IO_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_io_id() -> u32 {
    IO_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Kind of I/O operation performed through an instrumented wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IoOpKind {
    Read,
    Write,
    Flush,
    #[cfg_attr(not(feature = "tokio"), allow(dead_code))]
    Shutdown,
}

impl IoOpKind {
    /// Index into the per-kind thread-local sampling counters.
    fn sampling_idx(self) -> usize {
        match self {
            IoOpKind::Read => 0,
            IoOpKind::Write => 1,
            IoOpKind::Flush => 2,
            IoOpKind::Shutdown => 3,
        }
    }
}

/// Events sent to the background I/O statistics collection thread.
#[derive(Debug)]
pub(crate) enum IoEvent {
    Created {
        id: u32,
        source: &'static str,
        label: Option<String>,
        type_name: &'static str,
    },
    /// A completed operation. For async I/O the duration spans from the first
    /// poll of the operation to `Ready`, so it includes suspended time.
    /// `duration_nanos` and `started_at_ns` are `None` when timing was skipped
    /// under time sampling; the event still counts the operation and its bytes.
    Op {
        id: u32,
        kind: IoOpKind,
        bytes: u64,
        duration_nanos: Option<u64>,
        /// Operation start (ns since program start); anchors the rate window.
        started_at_ns: Option<u64>,
    },
    /// An operation that failed. Retryable conditions (`WouldBlock`,
    /// `Interrupted`) are not reported as errors.
    Error { id: u32, kind: IoOpKind },
}

/// Per-operation-kind statistics for a single instrumented I/O value.
#[derive(Debug, Clone)]
pub(crate) struct IoOpStats {
    pub(crate) count: u64,
    pub(crate) sampled_count: u64,
    pub(crate) bytes: u64,
    /// Bytes from timed operations only; numerator of the byte rate, so the
    /// rate stays unbiased under time sampling.
    pub(crate) sampled_bytes: u64,
    pub(crate) errors: u64,
    pub(crate) total_nanos: u64,
    /// Time with at least one timed operation in flight: the union of op
    /// intervals, so concurrent operations aggregated into one entry don't
    /// double-count overlapped time. Denominator of the byte rate.
    busy_nanos: u64,
    /// Watermark for the busy-interval union: end (ns since program start) of
    /// the latest counted interval.
    busy_until_ns: u64,
    hist: Option<Histogram<u64>>,
}

impl IoOpStats {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    fn new() -> Self {
        Self {
            count: 0,
            sampled_count: 0,
            bytes: 0,
            sampled_bytes: 0,
            errors: 0,
            total_nanos: 0,
            busy_nanos: 0,
            busy_until_ns: 0,
            hist: Some(
                Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
                    .expect("hdrhistogram init"),
            ),
        }
    }

    fn record(&mut self, bytes: u64, duration_nanos: Option<u64>, started_at_ns: Option<u64>) {
        self.count += 1;
        self.bytes += bytes;
        if let Some(nanos) = duration_nanos {
            self.sampled_count += 1;
            self.sampled_bytes += bytes;
            self.total_nanos += nanos;
            if let Some(start) = started_at_ns {
                // Watermark union: an op inside the already-counted region adds
                // nothing, a partially overlapping one adds only its extension.
                // Exact for ops processed in start order; see `flush_io_buffer`.
                let end = start.saturating_add(nanos);
                let counted_from = start.max(self.busy_until_ns);
                if end > counted_from {
                    self.busy_nanos += end - counted_from;
                    self.busy_until_ns = end;
                }
            }
            if let Some(ref mut hist) = self.hist {
                hist.record(nanos.clamp(Self::LOW_NS, Self::HIGH_NS))
                    .unwrap();
            }
        }
    }

    pub(crate) fn avg_nanos(&self) -> u64 {
        self.total_nanos
            .checked_div(self.sampled_count)
            .unwrap_or(0)
    }

    /// Bytes per second of active I/O time: `sampled_bytes` over the union of
    /// in-flight operation intervals. Idle gaps between operations don't
    /// dilute the rate, and overlapped time from concurrent operations
    /// aggregated into one entry is counted once, so the value reads as the
    /// entry's transfer speed while data was moving. `None` when nothing was
    /// timed.
    pub(crate) fn throughput_bytes_per_sec(&self) -> Option<f64> {
        if self.busy_nanos == 0 {
            return None;
        }
        Some(self.sampled_bytes as f64 * 1e9 / self.busy_nanos as f64)
    }

    pub(crate) fn percentile_nanos(&self, p: f64) -> u64 {
        match &self.hist {
            Some(hist) if self.sampled_count > 0 => hist.value_at_percentile(p.clamp(0.0, 100.0)),
            _ => 0,
        }
    }
}

/// Statistics for a single `io!` creation site (source location + concrete
/// type). All wrapper instances from that site accumulate into one entry.
#[derive(Debug, Clone)]
pub(crate) struct IoEntry {
    pub(crate) id: u32,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) type_name: &'static str,
    pub(crate) read: IoOpStats,
    pub(crate) write: IoOpStats,
    pub(crate) flush: IoOpStats,
    pub(crate) shutdown: IoOpStats,
    pub(crate) iter: u32,
}

impl IoEntry {
    pub(crate) fn op(&self, kind: IoOpKind) -> &IoOpStats {
        match kind {
            IoOpKind::Read => &self.read,
            IoOpKind::Write => &self.write,
            IoOpKind::Flush => &self.flush,
            IoOpKind::Shutdown => &self.shutdown,
        }
    }

    fn op_mut(&mut self, kind: IoOpKind) -> &mut IoOpStats {
        match kind {
            IoOpKind::Read => &mut self.read,
            IoOpKind::Write => &mut self.write,
            IoOpKind::Flush => &mut self.flush,
            IoOpKind::Shutdown => &mut self.shutdown,
        }
    }

    /// Errors across all write-side kinds, shown on the write sub-table row so
    /// flush failures (where buffered writers surface deferred errors) aren't
    /// hidden from the table.
    pub(crate) fn write_side_errors(&self) -> u64 {
        self.write.errors + self.flush.errors + self.shutdown.errors
    }
}

pub(crate) struct IoInternalState {
    pub(crate) stats: HashMap<u32, IoEntry>,
}

pub(crate) struct IoState {
    pub(crate) inner: Arc<StdRwLock<IoInternalState>>,
    pub(crate) shutdown_tx: StdMutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: StdMutex<Option<CbReceiver<()>>>,
}

pub(crate) static IO_STATE: OnceLock<IoState> = OnceLock::new();

pub(crate) fn get_sorted_io_entries() -> Vec<IoEntry> {
    let Some(state) = IO_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<IoEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_io_entries);
    stats
}

pub(crate) fn get_io_json() -> crate::json::JsonIoList {
    let entries = get_sorted_io_entries();
    let elapsed = std::time::Duration::from_nanos(crate::lib_on::current_elapsed_ns());
    crate::lib_on::report::collect_io_json(
        &entries,
        elapsed,
        &crate::lib_on::hotpath_guard::configured_percentiles(),
    )
}

#[inline]
pub(crate) fn elapsed_nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos() as u64
}

#[inline]
pub(crate) fn started_at_nanos(start: Instant) -> u64 {
    crate::lib_on::channels::timestamp_nanos(start)
}

/// One sampling decision per operation; `None` skips both clock reads. Each
/// operation kind samples from its own counter so periodic workloads can't
/// bias which kinds get timed.
#[inline]
pub(crate) fn op_stamp(kind: IoOpKind) -> Option<Instant> {
    crate::lib_on::sampling::io_should_time(kind.sampling_idx()).then(Instant::now)
}

/// Rolls back an `op_stamp` decision after an operation that produced no
/// measurement (retryable condition or error), so the sampling rate applies
/// to completed operations.
#[inline]
pub(crate) fn cancel_op_stamp(kind: IoOpKind) {
    crate::lib_on::sampling::io_untime(kind.sampling_idx());
}

static EVENT_QUEUES: EventQueueRegistry<IoEvent> = EventQueueRegistry::new();

thread_local! {
    static EVENT_PRODUCER: EventProducer<IoEvent> = EVENT_QUEUES.register();
}

#[inline]
pub(crate) fn send_io_event(event: IoEvent) {
    if !EVENT_QUEUES.is_active() {
        return;
    }
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    let _ = EVENT_PRODUCER.try_with(|producer| producer.push(event));
}

/// Stops producers ahead of the worker's final sweep at shutdown.
pub(crate) fn stop_io_events() {
    EVENT_QUEUES.set_active(false);
}

/// Entry for events that arrive ahead of their `Created` (sweeps only preserve
/// per-thread order, so another thread's data events can be drained first).
/// `Created` backfills the metadata.
fn placeholder_io_entry(id: u32) -> IoEntry {
    IoEntry {
        id,
        source: "",
        label: None,
        type_name: "",
        read: IoOpStats::new(),
        write: IoOpStats::new(),
        flush: IoOpStats::new(),
        shutdown: IoOpStats::new(),
        iter: 0,
    }
}

fn process_io_event(state: &mut IoInternalState, event: IoEvent) {
    match event {
        IoEvent::Created {
            id,
            source,
            label,
            type_name,
        } => {
            let iter = state.stats.values().filter(|s| s.source == source).count() as u32;
            let entry = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_io_entry(id));
            entry.source = source;
            entry.label = label;
            entry.type_name = type_name;
            entry.iter = iter;
        }
        IoEvent::Op {
            id,
            kind,
            bytes,
            duration_nanos,
            started_at_ns,
        } => {
            let entry = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_io_entry(id));
            entry
                .op_mut(kind)
                .record(bytes, duration_nanos, started_at_ns);
        }
        IoEvent::Error { id, kind } => {
            let entry = state
                .stats
                .entry(id)
                .or_insert_with(|| placeholder_io_entry(id));
            entry.op_mut(kind).errors += 1;
        }
    }
}

/// Entries are keyed by creation site and concrete type, so wrappers created
/// repeatedly at one `io!` call (e.g. per accepted connection in a server)
/// share a single accumulating entry and state stays bounded by the number of
/// call sites rather than the number of values ever wrapped.
type IoSourceKey = (&'static str, &'static str);

static IO_SOURCE_IDS: OnceLock<StdRwLock<HashMap<IoSourceKey, u32>>> = OnceLock::new();

/// Registers an instrumented I/O value, reusing the entry id of earlier
/// wrappers from the same call site and type. Only the first registration
/// emits `Created`, so its `label` wins.
pub(crate) fn register_io<T>(source: &'static str, label: Option<String>) -> u32 {
    let type_name = std::any::type_name::<T>();
    init_io_state();

    let map = IO_SOURCE_IDS.get_or_init(|| StdRwLock::new(HashMap::new()));
    if let Some(&id) = map.read().unwrap().get(&(source, type_name)) {
        return id;
    }
    let mut writer = map.write().unwrap();
    if let Some(&id) = writer.get(&(source, type_name)) {
        return id;
    }
    let id = next_io_id();
    writer.insert((source, type_name), id);

    send_io_event(IoEvent::Created {
        id,
        source,
        label,
        type_name,
    });

    id
}

fn flush_io_buffer(buffer: &mut Vec<IoEvent>, inner: &Arc<StdRwLock<IoInternalState>>) {
    if buffer.is_empty() {
        return;
    }
    // Per-thread queues are swept sequentially, so a batch interleaves ops
    // from different threads out of start order, which would undercount the
    // busy-interval union. Sorting timed ops by start makes the union exact
    // within a sweep (stable sort keeps the remaining events' relative order);
    // residual error across sweep boundaries is bounded by the drain interval.
    buffer.sort_by_key(|e| match e {
        IoEvent::Op {
            started_at_ns: Some(ns),
            ..
        } => *ns,
        _ => u64::MAX,
    });
    if let Ok(mut shared) = inner.write() {
        for e in buffer.drain(..) {
            process_io_event(&mut shared, e);
        }
    }
}

/// Initialize the I/O statistics collection system (called on first instrumented value).
pub(crate) fn init_io_state() -> &'static IoState {
    IO_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);

        let inner = Arc::new(StdRwLock::new(IoInternalState {
            stats: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        EVENT_QUEUES.set_active(true);

        std::thread::Builder::new()
            .name("hp-io".into())
            .spawn(move || {
                let flush_interval = std::time::Duration::from_millis(*DRAIN_INTERVAL_MS);
                let mut swept: Vec<IoEvent> = Vec::new();

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
                        flush_io_buffer(&mut swept, &inner_clone);
                        break;
                    }

                    EVENT_QUEUES.sweep(&mut swept);
                    flush_io_buffer(&mut swept, &inner_clone);
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn io-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        IoState {
            inner,
            shutdown_tx: StdMutex::new(Some(shutdown_tx)),
            completion_rx: StdMutex::new(Some(completion_rx)),
        }
    })
}

/// Compare two I/O stats for sorting. Custom labels first, then by source and iter.
pub(crate) fn compare_io_entries(a: &IoEntry, b: &IoEntry) -> std::cmp::Ordering {
    match (a.label.is_some(), b.label.is_some()) {
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

#[cfg(test)]
mod tests {
    use crate::lib_on::io::IoOpStats;

    #[test]
    fn busy_union_watermark() {
        let mut stats = IoOpStats::new();
        stats.record(100, Some(10), Some(0));
        stats.record(100, Some(15), Some(5));
        stats.record(100, Some(2), Some(6));
        stats.record(100, Some(10), Some(30));
        assert_eq!(stats.busy_nanos, 30);
        assert_eq!(stats.sampled_bytes, 400);

        stats.record(100, None, None);
        assert_eq!(stats.bytes, 500);
        assert_eq!(stats.busy_nanos, 30);

        let rate = stats.throughput_bytes_per_sec().unwrap();
        let expected = 400.0 * 1e9 / 30.0;
        assert!((rate - expected).abs() < 1.0);
    }

    #[test]
    fn throughput_none_when_untimed() {
        let mut stats = IoOpStats::new();
        stats.record(100, None, None);
        assert_eq!(stats.throughput_bytes_per_sec(), None);
    }
}

/// Instrument a value implementing [`std::io::Read`], [`std::io::Write`],
/// `tokio::io::AsyncRead`, or `tokio::io::AsyncWrite` for byte-level I/O
/// profiling (`tokio` traits require the `tokio` feature).
///
/// Returns an [`InstrumentedIo`] wrapper that delegates every operation to the
/// wrapped value and records operation counts, bytes processed, durations, and
/// errors. Synchronous reads and writes measure the full method call; async
/// operations measure from the first poll to `Ready`, so reported durations
/// include async waiting time.
///
/// Wrapping the underlying resource (file, socket) measures actual resource
/// I/O; wrapping a `BufReader`/`BufWriter` measures application-facing
/// buffered operations.
///
/// # Examples
///
/// ```rust,no_run
/// use std::io::Read;
///
/// let mut reader = hotpath::io!(std::io::Cursor::new(vec![1u8, 2, 3]), label = "cursor");
/// let mut buf = [0u8; 3];
/// reader.read_exact(&mut buf).unwrap();
/// ```
#[macro_export]
macro_rules! io {
    ($expr:expr) => {{
        const IO_ID: &'static str = concat!(file!(), ":", line!());
        $crate::io::InstrumentedIo::__new_instrumented($expr, IO_ID, None)
    }};

    ($expr:expr, label = $label:expr) => {{
        const IO_ID: &'static str = concat!(file!(), ":", line!());
        $crate::io::InstrumentedIo::__new_instrumented($expr, IO_ID, Some($label.to_string()))
    }};
}
