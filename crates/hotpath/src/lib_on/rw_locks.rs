//! RwLock instrumentation module - tracks read/write lock acquisitions and hold durations.

use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock as StdRwLock};

use crate::instant::Instant;
use crate::lib_on::hotpath_guard::{
    WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
use crate::lib_on::START_TIME;
use crate::metrics_server::METRICS_SERVER_PORT;

pub(crate) mod wrapper;

// Re-exported to keep the std wrapper reachable at `hotpath::rw_locks::*` for downstream code.
pub use wrapper::std::{RwLock, RwLockReadGuard, RwLockWriteGuard};

static RW_LOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_rw_lock_id() -> u32 {
    RW_LOCK_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Whether an acquisition was a shared (read) or exclusive (write) lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RwLockKind {
    Read,
    Write,
}

/// Events sent to the background lock statistics collection thread.
#[derive(Debug)]
pub(crate) enum RwLockEvent {
    Created {
        id: u32,
        source: &'static str,
        label: Option<String>,
        type_name: &'static str,
    },
    /// Emitted when a guard is dropped. `nanos` is the hold duration.
    Released {
        id: u32,
        kind: RwLockKind,
        nanos: u64,
    },
}

/// Handle returned by [`register_rw_lock`] giving a wrapper its id and a sender
/// to emit [`RwLockEvent`]s to the background worker.
pub(crate) struct RegisteredRwLock {
    pub(crate) id: u32,
    pub(crate) stats_tx: CbSender<RwLockEvent>,
}

/// Statistics for a single instrumented RwLock.
#[derive(Debug, Clone)]
pub(crate) struct RwLockEntry {
    pub(crate) id: u32,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) type_name: &'static str,
    pub(crate) read_count: u64,
    pub(crate) write_count: u64,
    pub(crate) read_total_nanos: u64,
    pub(crate) write_total_nanos: u64,
    read_hist: Option<Histogram<u64>>,
    write_hist: Option<Histogram<u64>>,
    pub(crate) iter: u32,
}

impl RwLockEntry {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    fn new_histogram() -> Histogram<u64> {
        Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
            .expect("hdrhistogram init")
    }

    #[inline]
    fn record_read(&mut self, nanos: u64) {
        if let Some(ref mut hist) = self.read_hist {
            hist.record(nanos.clamp(Self::LOW_NS, Self::HIGH_NS))
                .unwrap();
        }
    }

    #[inline]
    fn record_write(&mut self, nanos: u64) {
        if let Some(ref mut hist) = self.write_hist {
            hist.record(nanos.clamp(Self::LOW_NS, Self::HIGH_NS))
                .unwrap();
        }
    }

    pub(crate) fn read_avg_nanos(&self) -> u64 {
        self.read_total_nanos
            .checked_div(self.read_count)
            .unwrap_or(0)
    }

    pub(crate) fn write_avg_nanos(&self) -> u64 {
        self.write_total_nanos
            .checked_div(self.write_count)
            .unwrap_or(0)
    }

    pub(crate) fn read_percentile_nanos(&self, p: f64) -> u64 {
        match &self.read_hist {
            Some(hist) if self.read_count > 0 => hist.value_at_percentile(p.clamp(0.0, 100.0)),
            _ => 0,
        }
    }

    pub(crate) fn write_percentile_nanos(&self, p: f64) -> u64 {
        match &self.write_hist {
            Some(hist) if self.write_count > 0 => hist.value_at_percentile(p.clamp(0.0, 100.0)),
            _ => 0,
        }
    }
}

pub(crate) struct RwLocksInternalState {
    pub(crate) stats: HashMap<u32, RwLockEntry>,
}

pub(crate) struct RwLocksState {
    pub(crate) event_tx: CbSender<RwLockEvent>,
    pub(crate) inner: Arc<StdRwLock<RwLocksInternalState>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<()>>>,
}

pub(crate) static RW_LOCKS_STATE: OnceLock<RwLocksState> = OnceLock::new();

#[inline]
pub(crate) fn send_rw_lock_event(stats_tx: &CbSender<RwLockEvent>, event: RwLockEvent) {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    let _ = stats_tx.send(event);
}

fn process_rw_lock_event(state: &mut RwLocksInternalState, event: RwLockEvent) {
    match event {
        RwLockEvent::Created {
            id,
            source,
            label,
            type_name,
        } => {
            let iter = state.stats.values().filter(|s| s.source == source).count() as u32;
            state.stats.insert(
                id,
                RwLockEntry {
                    id,
                    source,
                    label,
                    type_name,
                    read_count: 0,
                    write_count: 0,
                    read_total_nanos: 0,
                    write_total_nanos: 0,
                    read_hist: Some(RwLockEntry::new_histogram()),
                    write_hist: Some(RwLockEntry::new_histogram()),
                    iter,
                },
            );
        }
        RwLockEvent::Released { id, kind, nanos } => {
            if let Some(entry) = state.stats.get_mut(&id) {
                match kind {
                    RwLockKind::Read => {
                        entry.read_count += 1;
                        entry.read_total_nanos += nanos;
                        entry.record_read(nanos);
                    }
                    RwLockKind::Write => {
                        entry.write_count += 1;
                        entry.write_total_nanos += nanos;
                        entry.record_write(nanos);
                    }
                }
            }
        }
    }
}

/// Registers a new RwLock with the profiling subsystem.
pub(crate) fn register_rw_lock<T>(source: &'static str, label: Option<String>) -> RegisteredRwLock {
    let type_name = std::any::type_name::<T>();
    let state = init_rw_locks_state();
    let id = next_rw_lock_id();

    send_rw_lock_event(
        &state.event_tx,
        RwLockEvent::Created {
            id,
            source,
            label,
            type_name,
        },
    );

    RegisteredRwLock {
        id,
        stats_tx: state.event_tx.clone(),
    }
}

/// Initialize the lock statistics collection system (called on first instrumented lock).
pub(crate) fn init_rw_locks_state() -> &'static RwLocksState {
    RW_LOCKS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (event_tx, event_rx) = unbounded::<RwLockEvent>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);

        let inner = Arc::new(StdRwLock::new(RwLocksInternalState {
            stats: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("hp-rw-locks".into())
            .spawn(move || {
                let mut local_buffer: Vec<RwLockEvent> = Vec::with_capacity(WORKER_BATCH_SIZE);
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
                                                process_rw_lock_event(&mut shared, e);
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    if !local_buffer.is_empty() {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_rw_lock_event(&mut shared, e);
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
                                    process_rw_lock_event(&mut shared, e);
                                }
                                for event in drained_events {
                                    process_rw_lock_event(&mut shared, event);
                                }
                            }
                            break;
                        }
                        default(flush_interval) => {
                            if !local_buffer.is_empty() {
                                if let Ok(mut shared) = inner_clone.write() {
                                    for e in local_buffer.drain(..) {
                                        process_rw_lock_event(&mut shared, e);
                                    }
                                }
                            }
                        }
                    }
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn rw_lock-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        RwLocksState {
            event_tx,
            inner,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
    })
}

/// Compare two lock stats for sorting. Custom labels first, then by source and iter.
pub(crate) fn compare_rw_lock_entries(a: &RwLockEntry, b: &RwLockEntry) -> std::cmp::Ordering {
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

/// Trait for instrumenting RwLocks. Dispatches on the type of the wrapped lock
/// (e.g. [`std::sync::RwLock`] or [`parking_lot::RwLock`]).
///
/// This trait is not intended for direct use. Use the `rw_lock!` macro instead.
#[doc(hidden)]
pub trait InstrumentRwLock {
    type Output;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output;
}

/// Instrument an [`std::sync::RwLock`] or [`parking_lot::RwLock`] for read/write profiling.
///
/// Returns an instrumented drop-in replacement that proxies to the wrapped lock and records
/// how long read and write locks are held. The wrapper type matches the API of the underlying
/// lock (`std::sync::RwLock` returns `LockResult`s; `parking_lot::RwLock` returns guards directly).
///
/// `parking_lot::RwLock` support requires the `parking_lot` feature.
///
/// # Examples
///
/// ```rust,no_run
/// let lock = hotpath::rw_lock!(std::sync::RwLock::new(0u32));
/// *lock.write().unwrap() += 1;
/// let _ = *lock.read().unwrap();
/// ```
#[macro_export]
macro_rules! rw_lock {
    ($expr:expr) => {{
        const RW_LOCK_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentRwLock::instrument($expr, RW_LOCK_ID, None)
    }};

    ($expr:expr, label = $label:expr) => {{
        const RW_LOCK_ID: &'static str = concat!(file!(), ":", line!());
        $crate::InstrumentRwLock::instrument($expr, RW_LOCK_ID, Some($label.to_string()))
    }};
}
