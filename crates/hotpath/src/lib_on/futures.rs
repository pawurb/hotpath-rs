//! Futures instrumentation module - tracks async Future lifecycle and poll statistics.

use crate::channels::{resolve_label, LOGS_LIMIT, START_TIME};
use crate::metrics_server::METRICS_SERVER_PORT;
use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use crate::data_flow::{WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT};

pub(crate) mod wrapper;

pub use wrapper::{InstrumentedFuture, InstrumentedFutureLog};

use crate::json::JsonFutureEntry;
pub(crate) use crate::json::{FutureLog, FutureLogsList, FutureState};
pub use crate::Format;

pub(crate) static FUTURE_CALL_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

use std::sync::LazyLock;

/// Thread-safe map from source location to future_id
static SOURCE_TO_FUTURE_ID: LazyLock<RwLock<HashMap<&'static str, u32>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get or create a future_id for a source location.
/// Returns (future_id, is_new) where is_new indicates if this is a newly created future.
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_or_create_future_id(source: &'static str) -> (u32, bool) {
    let map = &*SOURCE_TO_FUTURE_ID;

    {
        let read_guard = map.read().unwrap();
        if let Some(&future_id) = read_guard.get(source) {
            return (future_id, false);
        }
    }

    let mut write_guard = map.write().unwrap();

    if let Some(&future_id) = write_guard.get(source) {
        return (future_id, false);
    }

    let future_id = crate::data_flow::next_data_flow_id();
    write_guard.insert(source, future_id);
    (future_id, true)
}

/// Aggregated statistics for a source location.
#[derive(Debug, Clone)]
pub(crate) struct FutureEntry {
    pub(crate) id: u32,
    pub(crate) source: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) logs_count: u64,
    pub(crate) total_poll_count: u64,
    pub(crate) total_poll_duration_ns: u64,
    pub(crate) total_poll_alloc_bytes: Option<u64>,
    pub(crate) total_poll_alloc_count: Option<u64>,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl FutureEntry {
    fn new(id: u32, source: &'static str, label: Option<String>) -> Self {
        Self {
            id,
            source,
            label,
            logs_count: 0,
            total_poll_count: 0,
            total_poll_duration_ns: 0,
            total_poll_alloc_bytes: None,
            total_poll_alloc_count: None,
        }
    }

    pub(crate) fn total_polls(&self) -> u64 {
        self.total_poll_count
    }

    pub(crate) fn total_poll_duration_ns(&self) -> u64 {
        self.total_poll_duration_ns
    }

    pub(crate) fn total_poll_alloc_bytes(&self) -> Option<u64> {
        self.total_poll_alloc_bytes
    }

    pub(crate) fn total_poll_alloc_count(&self) -> Option<u64> {
        self.total_poll_alloc_count
    }
}

#[derive(Debug)]
pub(crate) struct FutureEntryLogs {
    pub(crate) logs: VecDeque<FutureLog>,
}

impl FutureEntryLogs {
    fn new() -> Self {
        Self {
            logs: VecDeque::with_capacity(*LOGS_LIMIT),
        }
    }

    fn find_call_mut(&mut self, id: u32) -> Option<&mut FutureLog> {
        self.logs.iter_mut().find(|c| c.id == id)
    }
}

pub(crate) struct FuturesInternalState {
    pub(crate) stats: HashMap<u32, FutureEntry>,
    pub(crate) logs: HashMap<u32, FutureEntryLogs>,
}

impl From<&FutureEntry> for JsonFutureEntry {
    fn from(stats: &FutureEntry) -> Self {
        let label = resolve_label(stats.source, stats.label.as_deref(), None);

        JsonFutureEntry {
            id: stats.id,
            source: stats.source.to_string(),
            label,
            has_custom_label: stats.label.is_some(),
            call_count: stats.logs_count,
            total_polls: stats.total_polls(),
            total_poll_duration_ns: stats.total_poll_duration_ns(),
            total_poll_alloc_bytes: stats.total_poll_alloc_bytes(),
            total_poll_alloc_count: stats.total_poll_alloc_count(),
        }
    }
}

/// Result of polling a future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PollResult {
    Pending,
    Ready,
}

/// Events emitted during the lifecycle of an instrumented future.
#[derive(Debug)]
pub(crate) enum FutureEvent {
    Created {
        future_id: u32,
        source: &'static str,
        display_label: Option<String>,
    },
    CallCreated {
        future_id: u32,
        call_id: u32,
    },
    Polled {
        future_id: u32,
        call_id: u32,
        result: PollResult,
        log_message: Option<String>,
        poll_duration_ns: u64,
        poll_alloc_bytes: Option<u64>,
        poll_alloc_count: Option<u64>,
    },
    Completed {
        future_id: u32,
        call_id: u32,
    },
    Cancelled {
        future_id: u32,
        call_id: u32,
    },
}

pub(crate) struct FuturesState {
    pub(crate) event_tx: CbSender<FutureEvent>,
    pub(crate) inner: Arc<RwLock<FuturesInternalState>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<()>>>,
}

pub(crate) type FuturesStatsState = FuturesState;

pub(crate) static FUTURES_STATE: OnceLock<FuturesStatsState> = OnceLock::new();

/// Initialize the futures event collection system (called on first instrumented future).
#[doc(hidden)]
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn init_futures_state() {
    FUTURES_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        let (event_tx, event_rx) = unbounded::<FutureEvent>();
        #[cfg(feature = "hotpath-meta")]
        let (event_tx, event_rx) =
            hotpath_meta::channel!((event_tx, event_rx), label = "hp-ft-events", log = true);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        #[cfg(feature = "hotpath-meta")]
        let (shutdown_tx, shutdown_rx) = hotpath_meta::channel!(
            (shutdown_tx, shutdown_rx),
            label = "hp-ft-shutdown",
            log = true
        );
        let (completion_tx, completion_rx) = bounded::<()>(1);
        #[cfg(feature = "hotpath-meta")]
        let (completion_tx, completion_rx) = hotpath_meta::channel!(
            (completion_tx, completion_rx),
            label = "hp-ft-completion",
            log = true
        );
        let inner = Arc::new(RwLock::new(FuturesInternalState {
            stats: HashMap::new(),
            logs: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("hp-futures".into())
            .spawn(move || {
                let mut local_buffer: Vec<FutureEvent> = Vec::with_capacity(WORKER_BATCH_SIZE);
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
                                                process_future_event(&mut shared, e);
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    if !local_buffer.is_empty() {
                                        if let Ok(mut shared) = inner_clone.write() {
                                            for e in local_buffer.drain(..) {
                                                process_future_event(&mut shared, e);
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
                                    process_future_event(&mut shared, e);
                                }
                                for event in drained_events {
                                    process_future_event(&mut shared, event);
                                }
                            }
                            break;
                        }
                        default(flush_interval) => {
                            if !local_buffer.is_empty() {
                                if let Ok(mut shared) = inner_clone.write() {
                                    for e in local_buffer.drain(..) {
                                        process_future_event(&mut shared, e);
                                    }
                                }
                            }
                        }
                    }
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn futures event collector thread");

        FuturesState {
            event_tx,
            inner,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
    });
}

/// Process a future event and update stats.
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn process_future_event(state: &mut FuturesInternalState, event: FutureEvent) {
    fn add_optional(total: &mut Option<u64>, delta: Option<u64>) {
        if let Some(delta) = delta {
            *total = Some(total.unwrap_or(0) + delta);
        }
    }

    match event {
        FutureEvent::Created {
            future_id,
            source,
            display_label,
        } => {
            state.stats.insert(
                future_id,
                FutureEntry::new(future_id, source, display_label),
            );
            state.logs.insert(future_id, FutureEntryLogs::new());
        }
        FutureEvent::CallCreated { future_id, call_id } => {
            if let Some(future_stats) = state.stats.get_mut(&future_id) {
                future_stats.logs_count += 1;
            }
            if let Some(entry_logs) = state.logs.get_mut(&future_id) {
                let limit = *LOGS_LIMIT;
                if entry_logs.logs.len() >= limit {
                    entry_logs.logs.pop_front();
                }
                entry_logs
                    .logs
                    .push_back(FutureLog::new(call_id, future_id));
            }
        }
        FutureEvent::Polled {
            future_id,
            call_id,
            result,
            log_message,
            poll_duration_ns,
            poll_alloc_bytes,
            poll_alloc_count,
        } => {
            if let Some(future_stats) = state.stats.get_mut(&future_id) {
                future_stats.total_poll_count += 1;
                future_stats.total_poll_duration_ns += poll_duration_ns;
                add_optional(&mut future_stats.total_poll_alloc_bytes, poll_alloc_bytes);
                add_optional(&mut future_stats.total_poll_alloc_count, poll_alloc_count);
            }
            if let Some(entry_logs) = state.logs.get_mut(&future_id) {
                if let Some(call) = entry_logs.find_call_mut(call_id) {
                    call.poll_count += 1;
                    call.total_poll_duration_ns += poll_duration_ns;
                    call.last_poll_duration_ns = poll_duration_ns;
                    add_optional(&mut call.total_poll_alloc_bytes, poll_alloc_bytes);
                    add_optional(&mut call.total_poll_alloc_count, poll_alloc_count);
                    call.last_poll_alloc_bytes = poll_alloc_bytes;
                    if poll_duration_ns > call.max_poll_duration_ns {
                        call.max_poll_duration_ns = poll_duration_ns;
                    }
                    if let Some(poll_alloc_bytes) = poll_alloc_bytes {
                        if call
                            .max_poll_alloc_bytes
                            .is_none_or(|max| poll_alloc_bytes > max)
                        {
                            call.max_poll_alloc_bytes = Some(poll_alloc_bytes);
                        }
                    }
                    match result {
                        PollResult::Pending => {
                            call.state = FutureState::Suspended;
                        }
                        PollResult::Ready => {
                            call.state = FutureState::Ready;
                            if log_message.is_some() {
                                call.result = log_message;
                            }
                        }
                    };
                }
            }
        }
        FutureEvent::Completed { future_id, call_id } => {
            if let Some(entry_logs) = state.logs.get_mut(&future_id) {
                if let Some(call) = entry_logs.find_call_mut(call_id) {
                    call.state = FutureState::Ready;
                }
            }
        }
        FutureEvent::Cancelled { future_id, call_id } => {
            if let Some(entry_logs) = state.logs.get_mut(&future_id) {
                if let Some(call) = entry_logs.find_call_mut(call_id) {
                    if call.state != FutureState::Ready {
                        call.state = FutureState::Cancelled;
                    }
                }
            }
        }
    }
}

/// Send a future event to the background thread.
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn send_future_event(visible: bool, event: FutureEvent) {
    if !visible {
        return;
    };

    if let Some(state) = FUTURES_STATE.get() {
        let _ = state.event_tx.send(event);
    }
}

/// Trait for instrumenting futures (no Debug requirement).
///
/// This trait is not intended for direct use. Use the `future!` macro instead.
#[doc(hidden)]
pub trait InstrumentFuture {
    type Output;
    fn instrument_future(self, source: &'static str, label: Option<String>) -> Self::Output;
}

/// Trait for instrumenting futures with output logging (requires Debug).
///
/// This trait is not intended for direct use. Use the `future!` macro with `log = true` instead.
#[doc(hidden)]
pub trait InstrumentFutureLog {
    type Output;
    fn instrument_future_log(self, source: &'static str, label: Option<String>) -> Self::Output;
}

impl<F: std::future::Future> InstrumentFuture for F {
    type Output = InstrumentedFuture<F>;

    fn instrument_future(self, source: &'static str, label: Option<String>) -> Self::Output {
        InstrumentedFuture::new(self, source, label, None, true)
    }
}

impl<F: std::future::Future> InstrumentFutureLog for F
where
    F::Output: std::fmt::Debug,
{
    type Output = InstrumentedFutureLog<F>;

    fn instrument_future_log(self, source: &'static str, label: Option<String>) -> Self::Output {
        InstrumentedFutureLog::new(self, source, label, None, true)
    }
}

/// Compare two future stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source).
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn compare_future_stats(a: &FutureEntry, b: &FutureEntry) -> std::cmp::Ordering {
    let a_has_label = a.label.is_some();
    let b_has_label = b.label.is_some();

    match (a_has_label, b_has_label) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a.label.as_ref().unwrap().cmp(b.label.as_ref().unwrap()),
        (false, false) => a.source.cmp(b.source),
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_sorted_future_stats() -> Vec<FutureEntry> {
    let Some(state) = FUTURES_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<FutureEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_future_stats);
    stats
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_future_logs_list(future_id: u32) -> Option<FutureLogsList> {
    let state = FUTURES_STATE.get()?;
    let guard = state.inner.read().unwrap();
    let stats = guard.stats.get(&future_id)?;
    let entry_logs = guard.logs.get(&future_id)?;
    Some(FutureLogsList {
        id: future_id.to_string(),
        call_count: stats.logs_count,
        total_polls: stats.total_polls(),
        total_poll_duration_ns: stats.total_poll_duration_ns(),
        total_poll_alloc_bytes: stats.total_poll_alloc_bytes(),
        total_poll_alloc_count: stats.total_poll_alloc_count(),
        calls: entry_logs.logs.iter().rev().cloned().collect(),
    })
}

/// Instrument a future to inspect future's lifecycle events.
///
/// # Variants
///
/// - `future!(expr)` - No Debug requirement, prints `Ready` without the value
/// - `future!(expr, log = true)` - Requires Debug, prints `Ready(value)`
///
/// # Examples
///
/// ```rust,ignore
/// use hotpath::future;
///
/// // Without logging (works with any output type)
/// let result = future!(async { NoDebugType::new() }).await;
///
/// // With logging (requires Debug on output type)
/// let result = future!(async { 42 }, log = true).await;
/// ```
#[macro_export]
macro_rules! future {
    ($fut:expr) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFuture::instrument_future($fut, FUTURE_LOC, None)
    }};

    ($fut:expr, label = $label:expr) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFuture::instrument_future($fut, FUTURE_LOC, Some($label.to_string()))
    }};

    ($fut:expr, log = true) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFutureLog::instrument_future_log($fut, FUTURE_LOC, None)
    }};

    ($fut:expr, label = $label:expr, log = true) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFutureLog::instrument_future_log(
            $fut,
            FUTURE_LOC,
            Some($label.to_string()),
        )
    }};

    ($fut:expr, log = true, label = $label:expr) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFutureLog::instrument_future_log(
            $fut,
            FUTURE_LOC,
            Some($label.to_string()),
        )
    }};
}
