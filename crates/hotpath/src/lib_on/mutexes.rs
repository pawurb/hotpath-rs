//! Mutex instrumentation module - tracks lock acquisitions and hold durations.
//!
//! Unlike [`crate::rw_locks`], a mutex has a single lock kind (no read/write
//! distinction), so each entry tracks one set of wait & acquire statistics.

use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, RwLock as StdRwLock};

use crate::batch::{register_thread_batch, BatchRegistry, BatchedMeasurement, MeasurementBatch};
use crate::instant::Instant;
use crate::lib_on::hotpath_guard::{
    WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
use crate::lib_on::START_TIME;
use crate::metrics_server::METRICS_SERVER_PORT;

pub(crate) mod wrapper;

// Re-exported to keep the std wrapper reachable at `hotpath::mutexes::*` for downstream code.
pub use wrapper::std::{Mutex, MutexGuard};

static MUTEX_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_mutex_id() -> u32 {
    MUTEX_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Events sent to the background lock statistics collection thread.
#[derive(Debug)]
pub(crate) enum MutexEvent {
    Created {
        id: u32,
        source: &'static str,
        label: Option<String>,
        type_name: &'static str,
    },
    /// Emitted when a guard is dropped. `wait_nanos` is the time blocked
    /// before the lock was granted; `acquire_nanos` is the held duration
    /// (granted -> released).
    Released {
        id: u32,
        wait_nanos: u64,
        acquire_nanos: u64,
        elapsed_ns: u64,
    },
}

/// Statistics for a single instrumented Mutex.
#[derive(Debug, Clone)]
pub(crate) struct MutexEntry {
    pub(crate) id: u32,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) type_name: &'static str,
    pub(crate) count: u64,
    pub(crate) wait_total_nanos: u64,
    pub(crate) acquire_total_nanos: u64,
    wait_hist: Option<Histogram<u64>>,
    acquire_hist: Option<Histogram<u64>>,
    pub(crate) iter: u32,
}

impl MutexEntry {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    fn new_histogram() -> Histogram<u64> {
        Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
            .expect("hdrhistogram init")
    }

    #[inline]
    fn record(hist: &mut Option<Histogram<u64>>, nanos: u64) {
        if let Some(ref mut hist) = hist {
            hist.record(nanos.clamp(Self::LOW_NS, Self::HIGH_NS))
                .unwrap();
        }
    }

    pub(crate) fn wait_avg_nanos(&self) -> u64 {
        self.wait_total_nanos.checked_div(self.count).unwrap_or(0)
    }

    pub(crate) fn acquire_avg_nanos(&self) -> u64 {
        self.acquire_total_nanos
            .checked_div(self.count)
            .unwrap_or(0)
    }

    fn percentile(hist: &Option<Histogram<u64>>, count: u64, p: f64) -> u64 {
        match hist {
            Some(hist) if count > 0 => hist.value_at_percentile(p.clamp(0.0, 100.0)),
            _ => 0,
        }
    }

    pub(crate) fn wait_percentile_nanos(&self, p: f64) -> u64 {
        Self::percentile(&self.wait_hist, self.count, p)
    }

    pub(crate) fn acquire_percentile_nanos(&self, p: f64) -> u64 {
        Self::percentile(&self.acquire_hist, self.count, p)
    }
}

pub(crate) struct MutexesInternalState {
    pub(crate) stats: HashMap<u32, MutexEntry>,
}

pub(crate) struct MutexesState {
    pub(crate) event_tx: CbSender<Vec<MutexEvent>>,
    pub(crate) inner: Arc<StdRwLock<MutexesInternalState>>,
    pub(crate) shutdown_tx: StdMutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: StdMutex<Option<CbReceiver<()>>>,
}

pub(crate) static MUTEXES_STATE: OnceLock<MutexesState> = OnceLock::new();

pub(crate) fn get_sorted_mutex_entries() -> Vec<MutexEntry> {
    let Some(state) = MUTEXES_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<MutexEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_mutex_entries);
    stats
}

pub(crate) fn get_mutexes_json() -> crate::json::JsonMutexesList {
    let entries = get_sorted_mutex_entries();
    let elapsed = std::time::Duration::from_nanos(crate::lib_on::current_elapsed_ns());
    crate::lib_on::report::collect_mutexes_json(
        &entries,
        elapsed,
        &crate::lib_on::hotpath_guard::configured_percentiles(),
    )
}

#[inline]
pub(crate) fn elapsed_nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos() as u64
}

static EVENT_REGISTRY: BatchRegistry<MutexEvent> = BatchRegistry::new();

thread_local! {
    static EVENT_BATCH: std::sync::Arc<std::sync::Mutex<MeasurementBatch<MutexEvent>>> =
        register_thread_batch(&EVENT_REGISTRY);
}

#[inline]
pub(crate) fn send_mutex_event(event: MutexEvent) {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    EVENT_BATCH.with(|b| {
        if let Ok(mut b) = b.lock() {
            b.add(event);
        }
    });
}

/// Flushes every thread's buffered mutex events into the worker channel.
/// Called at shutdown before the worker is signalled to stop.
pub(crate) fn flush_mutex_batch() {
    EVENT_REGISTRY.flush_all();
}

impl BatchedMeasurement for MutexEvent {
    fn elapsed_since_start_ns(&self) -> u64 {
        match self {
            MutexEvent::Released { elapsed_ns, .. } => *elapsed_ns,
            _ => 0,
        }
    }

    fn fetch_sender() -> Option<CbSender<Vec<Self>>> {
        Some(MUTEXES_STATE.get()?.event_tx.clone())
    }

    fn is_flush_boundary(&self) -> bool {
        matches!(self, MutexEvent::Created { .. })
    }
}

fn process_mutex_event(state: &mut MutexesInternalState, event: MutexEvent) {
    match event {
        MutexEvent::Created {
            id,
            source,
            label,
            type_name,
        } => {
            let iter = state.stats.values().filter(|s| s.source == source).count() as u32;
            state.stats.insert(
                id,
                MutexEntry {
                    id,
                    source,
                    label,
                    type_name,
                    count: 0,
                    wait_total_nanos: 0,
                    acquire_total_nanos: 0,
                    wait_hist: Some(MutexEntry::new_histogram()),
                    acquire_hist: Some(MutexEntry::new_histogram()),
                    iter,
                },
            );
        }
        MutexEvent::Released {
            id,
            wait_nanos,
            acquire_nanos,
            elapsed_ns: _,
        } => {
            if let Some(entry) = state.stats.get_mut(&id) {
                entry.count += 1;
                entry.wait_total_nanos += wait_nanos;
                entry.acquire_total_nanos += acquire_nanos;
                MutexEntry::record(&mut entry.wait_hist, wait_nanos);
                MutexEntry::record(&mut entry.acquire_hist, acquire_nanos);
            }
        }
    }
}

/// Registers a new Mutex with the profiling subsystem.
pub(crate) fn register_mutex<T>(source: &'static str, label: Option<String>) -> u32 {
    let type_name = std::any::type_name::<T>();
    init_mutexes_state();
    let id = next_mutex_id();

    send_mutex_event(MutexEvent::Created {
        id,
        source,
        label,
        type_name,
    });

    id
}

/// Initialize the lock statistics collection system (called on first instrumented lock).
pub(crate) fn init_mutexes_state() -> &'static MutexesState {
    MUTEXES_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (event_tx, event_rx) = unbounded::<Vec<MutexEvent>>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);

        let inner = Arc::new(StdRwLock::new(MutexesInternalState {
            stats: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("hp-mutexes".into())
            .spawn(move || {
                let mut local_buffer: Vec<MutexEvent> = Vec::with_capacity(WORKER_BATCH_SIZE);
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
                                                process_mutex_event(&mut shared, e);
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    if !local_buffer.is_empty() {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_mutex_event(&mut shared, e);
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
                                    process_mutex_event(&mut shared, e);
                                }
                                for event in drained_events {
                                    process_mutex_event(&mut shared, event);
                                }
                            }
                            break;
                        }
                        default(flush_interval) => {
                            if !local_buffer.is_empty() {
                                if let Ok(mut shared) = inner_clone.write() {
                                    for e in local_buffer.drain(..) {
                                        process_mutex_event(&mut shared, e);
                                    }
                                }
                            }
                        }
                    }
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn mutex-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        MutexesState {
            event_tx,
            inner,
            shutdown_tx: StdMutex::new(Some(shutdown_tx)),
            completion_rx: StdMutex::new(Some(completion_rx)),
        }
    })
}

/// Compare two lock stats for sorting. Custom labels first, then by source and iter.
pub(crate) fn compare_mutex_entries(a: &MutexEntry, b: &MutexEntry) -> std::cmp::Ordering {
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

/// Trait for instrumenting Mutexes. Dispatches on the type of the wrapped lock
/// (e.g. [`std::sync::Mutex`]).
///
/// This trait is not intended for direct use. Use the `mutex!` macro instead.
#[doc(hidden)]
pub trait InstrumentMutex {
    type Output;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output;
}

/// Instrument an [`std::sync::Mutex`], `tokio::sync::Mutex`, or `async_lock::Mutex` for lock
/// wait & acquire profiling.
///
/// Returns an instrumented drop-in replacement that proxies to the wrapped lock and records
/// how long the lock is waited for and held. The wrapper type matches the API of the underlying
/// lock (`std::sync::Mutex` returns `LockResult`s; `tokio::sync::Mutex` and `async_lock::Mutex`
/// expose an async `lock` returning the guard directly).
///
/// `tokio::sync::Mutex` support requires the `tokio` feature; `async_lock::Mutex` support
/// requires the `async-lock` feature.
///
/// # Examples
///
/// ```rust,no_run
/// let lock = hotpath::mutex!(std::sync::Mutex::new(0u32));
/// *lock.lock().unwrap() += 1;
/// ```
#[macro_export]
macro_rules! mutex {
    ($expr:expr) => {{
        const MUTEX_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentMutex::instrument($expr, MUTEX_ID, None)
    }};

    ($expr:expr, label = $label:expr) => {{
        const MUTEX_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentMutex::instrument($expr, MUTEX_ID, Some($label.to_string()))
    }};
}
