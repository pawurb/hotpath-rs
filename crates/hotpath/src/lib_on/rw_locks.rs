//! RwLock instrumentation module - tracks read/write lock acquisitions and hold durations.

use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock as StdRwLock};

use crate::instant::Instant;
use crate::json::JsonRwLockEntry;
use crate::lib_on::hotpath_guard::{
    WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
use crate::lib_on::START_TIME;
use crate::metrics_server::METRICS_SERVER_PORT;

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
    pub(crate) read_max_nanos: u64,
    pub(crate) write_max_nanos: u64,
    pub(crate) iter: u32,
}

impl RwLockEntry {
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
}

impl From<&RwLockEntry> for JsonRwLockEntry {
    fn from(stats: &RwLockEntry) -> Self {
        let label =
            crate::channels::resolve_label(stats.source, stats.label.as_deref(), Some(stats.iter));

        JsonRwLockEntry {
            id: stats.id,
            source: stats.source.to_string(),
            label,
            has_custom_label: stats.label.is_some(),
            type_name: stats.type_name.to_string(),
            read_count: stats.read_count,
            write_count: stats.write_count,
            read_avg: crate::output::format_duration(stats.read_avg_nanos()),
            write_avg: crate::output::format_duration(stats.write_avg_nanos()),
            read_max: crate::output::format_duration(stats.read_max_nanos),
            write_max: crate::output::format_duration(stats.write_max_nanos),
            iter: stats.iter,
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
                    read_max_nanos: 0,
                    write_max_nanos: 0,
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
                        entry.read_max_nanos = entry.read_max_nanos.max(nanos);
                    }
                    RwLockKind::Write => {
                        entry.write_count += 1;
                        entry.write_total_nanos += nanos;
                        entry.write_max_nanos = entry.write_max_nanos.max(nanos);
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

/// Instrumented drop-in replacement for [`std::sync::RwLock`].
///
/// Not constructed directly - use the [`rw_lock!`](crate::rw_lock) macro.
pub struct RwLock<T> {
    inner: StdRwLock<T>,
    id: u32,
    stats_tx: CbSender<RwLockEvent>,
}

impl<T> RwLock<T> {
    #[doc(hidden)]
    pub fn __new_instrumented(
        inner: StdRwLock<T>,
        source: &'static str,
        label: Option<String>,
    ) -> Self {
        let RegisteredRwLock { id, stats_tx } = register_rw_lock::<T>(source, label);
        Self {
            inner,
            id,
            stats_tx,
        }
    }

    pub fn read(&self) -> std::sync::LockResult<HotpathReadGuard<'_, T>> {
        // Stamp the clock after acquisition so the guard measures hold time, not wait time.
        match self.inner.read() {
            Ok(inner) => Ok(self.read_guard(inner)),
            Err(poison) => Err(std::sync::PoisonError::new(
                self.read_guard(poison.into_inner()),
            )),
        }
    }

    pub fn try_read(&self) -> std::sync::TryLockResult<HotpathReadGuard<'_, T>> {
        match self.inner.try_read() {
            Ok(inner) => Ok(self.read_guard(inner)),
            Err(std::sync::TryLockError::Poisoned(poison)) => {
                Err(std::sync::TryLockError::Poisoned(
                    std::sync::PoisonError::new(self.read_guard(poison.into_inner())),
                ))
            }
            Err(std::sync::TryLockError::WouldBlock) => Err(std::sync::TryLockError::WouldBlock),
        }
    }

    pub fn write(&self) -> std::sync::LockResult<HotpathWriteGuard<'_, T>> {
        match self.inner.write() {
            Ok(inner) => Ok(self.write_guard(inner)),
            Err(poison) => Err(std::sync::PoisonError::new(
                self.write_guard(poison.into_inner()),
            )),
        }
    }

    pub fn try_write(&self) -> std::sync::TryLockResult<HotpathWriteGuard<'_, T>> {
        match self.inner.try_write() {
            Ok(inner) => Ok(self.write_guard(inner)),
            Err(std::sync::TryLockError::Poisoned(poison)) => {
                Err(std::sync::TryLockError::Poisoned(
                    std::sync::PoisonError::new(self.write_guard(poison.into_inner())),
                ))
            }
            Err(std::sync::TryLockError::WouldBlock) => Err(std::sync::TryLockError::WouldBlock),
        }
    }

    pub fn into_inner(self) -> std::sync::LockResult<T> {
        self.inner.into_inner()
    }

    pub fn get_mut(&mut self) -> std::sync::LockResult<&mut T> {
        self.inner.get_mut()
    }

    fn read_guard<'a>(&self, inner: std::sync::RwLockReadGuard<'a, T>) -> HotpathReadGuard<'a, T> {
        HotpathReadGuard {
            inner,
            start: Instant::now(),
            id: self.id,
            stats_tx: self.stats_tx.clone(),
        }
    }

    fn write_guard<'a>(
        &self,
        inner: std::sync::RwLockWriteGuard<'a, T>,
    ) -> HotpathWriteGuard<'a, T> {
        HotpathWriteGuard {
            inner,
            start: Instant::now(),
            id: self.id,
            stats_tx: self.stats_tx.clone(),
        }
    }
}

/// Guard returned by [`RwLock::read`]. Emits the hold duration on drop.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct HotpathReadGuard<'a, T> {
    inner: std::sync::RwLockReadGuard<'a, T>,
    start: Instant,
    id: u32,
    stats_tx: CbSender<RwLockEvent>,
}

impl<T> std::ops::Deref for HotpathReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> Drop for HotpathReadGuard<'_, T> {
    fn drop(&mut self) {
        let nanos = self.start.elapsed().as_nanos() as u64;
        send_rw_lock_event(
            &self.stats_tx,
            RwLockEvent::Released {
                id: self.id,
                kind: RwLockKind::Read,
                nanos,
            },
        );
    }
}

/// Guard returned by [`RwLock::write`]. Emits the hold duration on drop.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct HotpathWriteGuard<'a, T> {
    inner: std::sync::RwLockWriteGuard<'a, T>,
    start: Instant,
    id: u32,
    stats_tx: CbSender<RwLockEvent>,
}

impl<T> std::ops::Deref for HotpathWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for HotpathWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Drop for HotpathWriteGuard<'_, T> {
    fn drop(&mut self) {
        let nanos = self.start.elapsed().as_nanos() as u64;
        send_rw_lock_event(
            &self.stats_tx,
            RwLockEvent::Released {
                id: self.id,
                kind: RwLockKind::Write,
                nanos,
            },
        );
    }
}

/// Instrument an [`std::sync::RwLock`] for read/write profiling.
///
/// Returns a [`hotpath::wrap::std::sync::RwLock`](crate::wrap::std::sync::RwLock) that proxies
/// to the wrapped lock and records how long read and write locks are held.
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
        $crate::wrap::std::sync::RwLock::__new_instrumented($expr, RW_LOCK_ID, None)
    }};

    ($expr:expr, label = $label:expr) => {{
        const RW_LOCK_ID: &'static str = concat!(file!(), ":", line!());
        $crate::wrap::std::sync::RwLock::__new_instrumented(
            $expr,
            RW_LOCK_ID,
            Some($label.to_string()),
        )
    }};
}
