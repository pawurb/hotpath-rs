//! Futures instrumentation module - tracks async Future lifecycle and poll statistics.

use crate::channels::{resolve_label, LOG_LIMIT, START_TIME};
use crate::metrics_server::METRICS_SERVER_PORT;
use crossbeam_channel::{bounded, select, unbounded, Receiver as CbReceiver, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod guard;
pub(crate) mod wrapper;

pub use guard::{FuturesGuard, FuturesGuardBuilder};
pub use wrapper::{InstrumentedFuture, InstrumentedFutureLog};

pub use crate::json::{FutureLog, FutureLogsList, FutureState};
use crate::json::{JsonFutureEntry, JsonFuturesList};
pub use crate::Format;

pub(crate) static FUTURE_CALL_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

use std::sync::LazyLock;

/// Thread-safe map from source location to future_id
static SOURCE_TO_FUTURE_ID: LazyLock<RwLock<HashMap<&'static str, u64>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get or create a future_id for a source location.
/// Returns (future_id, is_new) where is_new indicates if this is a newly created future.
pub(crate) fn get_or_create_future_id(source: &'static str) -> (u64, bool) {
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
pub struct FutureEntry {
    pub id: u64,
    pub source: &'static str,
    pub label: Option<String>,
    pub logs: VecDeque<FutureLog>,
    pub logs_count: u64,
}

impl FutureEntry {
    fn new(id: u64, source: &'static str, label: Option<String>) -> Self {
        Self {
            id,
            source,
            label,
            logs: VecDeque::new(),
            logs_count: 0,
        }
    }

    /// Total polls across all invocations
    pub fn total_polls(&self) -> u64 {
        self.logs.iter().map(|c| c.poll_count).sum()
    }

    /// Find a call by ID
    fn find_call_mut(&mut self, id: u64) -> Option<&mut FutureLog> {
        self.logs.iter_mut().find(|c| c.id == id)
    }
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
        future_id: u64,
        source: &'static str,
        display_label: Option<String>,
    },
    CallCreated {
        future_id: u64,
        call_id: u64,
    },
    Polled {
        future_id: u64,
        call_id: u64,
        result: PollResult,
        log_message: Option<String>,
    },
    Completed {
        future_id: u64,
        call_id: u64,
    },
    Cancelled {
        future_id: u64,
        call_id: u64,
    },
}

pub(crate) struct FuturesState {
    pub(crate) event_tx: CbSender<FutureEvent>,
    pub(crate) stats_map: Arc<RwLock<HashMap<u64, FutureEntry>>>,
    pub(crate) shutdown_tx: Mutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: Mutex<Option<CbReceiver<HashMap<u64, FutureEntry>>>>,
}

pub(crate) type FuturesStatsState = FuturesState;

pub(crate) static FUTURES_STATE: OnceLock<FuturesStatsState> = OnceLock::new();

/// Initialize the futures event collection system (called on first instrumented future).
#[doc(hidden)]
pub fn init_futures_state() {
    FUTURES_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        let (event_tx, event_rx) = unbounded::<FutureEvent>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<HashMap<u64, FutureEntry>>(1);
        let stats_map = Arc::new(RwLock::new(HashMap::<u64, FutureEntry>::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-meta-futures".into())
            .spawn(move || {
                let mut local_stats = HashMap::<u64, FutureEntry>::new();

                loop {
                    select! {
                        recv(event_rx) -> result => {
                            match result {
                                Ok(event) => {
                                    process_future_event(&mut local_stats, event);
                                    if let Ok(mut shared) = stats_map_clone.write() {
                                        *shared = local_stats.clone();
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        recv(shutdown_rx) -> _ => {
                            while let Ok(event) = event_rx.try_recv() {
                                process_future_event(&mut local_stats, event);
                            }
                            break;
                        }
                    }
                }

                let _ = completion_tx.send(local_stats);
            })
            .expect("Failed to spawn futures event collector thread");

        FuturesState {
            event_tx,
            stats_map,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
    });
}

/// Process a future event and update stats.
fn process_future_event(stats_map: &mut HashMap<u64, FutureEntry>, event: FutureEvent) {
    match event {
        FutureEvent::Created {
            future_id,
            source,
            display_label,
        } => {
            stats_map.insert(
                future_id,
                FutureEntry::new(future_id, source, display_label),
            );
        }
        FutureEvent::CallCreated { future_id, call_id } => {
            if let Some(future_stats) = stats_map.get_mut(&future_id) {
                future_stats.logs_count += 1;
                let limit = *LOG_LIMIT;
                if future_stats.logs.len() >= limit {
                    future_stats.logs.pop_front();
                }
                future_stats
                    .logs
                    .push_back(FutureLog::new(call_id, future_id));
            }
        }
        FutureEvent::Polled {
            future_id,
            call_id,
            result,
            log_message,
        } => {
            if let Some(future_stats) = stats_map.get_mut(&future_id) {
                if let Some(call) = future_stats.find_call_mut(call_id) {
                    call.poll_count += 1;
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
            if let Some(future_stats) = stats_map.get_mut(&future_id) {
                if let Some(call) = future_stats.find_call_mut(call_id) {
                    call.state = FutureState::Ready;
                }
            }
        }
        FutureEvent::Cancelled { future_id, call_id } => {
            if let Some(future_stats) = stats_map.get_mut(&future_id) {
                if let Some(call) = future_stats.find_call_mut(call_id) {
                    if call.state != FutureState::Ready {
                        call.state = FutureState::Cancelled;
                    }
                }
            }
        }
    }
}

/// Send a future event to the background thread.
pub(crate) fn send_future_event(event: FutureEvent) {
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
    fn instrument_future(self, source: &'static str) -> Self::Output;
}

/// Trait for instrumenting futures with output logging (requires Debug).
///
/// This trait is not intended for direct use. Use the `future!` macro with `log = true` instead.
#[doc(hidden)]
pub trait InstrumentFutureLog {
    type Output;
    fn instrument_future_log(self, source: &'static str) -> Self::Output;
}

impl<F: std::future::Future> InstrumentFuture for F {
    type Output = InstrumentedFuture<F>;

    fn instrument_future(self, source: &'static str) -> Self::Output {
        InstrumentedFuture::new(self, source)
    }
}

impl<F: std::future::Future> InstrumentFutureLog for F
where
    F::Output: std::fmt::Debug,
{
    type Output = InstrumentedFutureLog<F>;

    fn instrument_future_log(self, source: &'static str) -> Self::Output {
        InstrumentedFutureLog::new(self, source)
    }
}

/// Compare two future stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source).
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

fn get_all_future_stats() -> HashMap<u64, FutureEntry> {
    if let Some(state) = FUTURES_STATE.get() {
        state.stats_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

pub(crate) fn get_sorted_future_stats() -> Vec<FutureEntry> {
    let mut stats: Vec<FutureEntry> = get_all_future_stats().into_values().collect();
    stats.sort_by(compare_future_stats);
    stats
}

pub fn get_futures_json() -> JsonFuturesList {
    let futures = get_sorted_future_stats()
        .iter()
        .map(JsonFutureEntry::from)
        .collect();

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    JsonFuturesList {
        current_elapsed_ns,
        futures,
    }
}

pub fn get_future_logs_list(future_id: u64) -> Option<FutureLogsList> {
    let stats = get_all_future_stats();
    stats.get(&future_id).map(|s| FutureLogsList {
        id: future_id.to_string(),
        calls: s.logs.iter().rev().cloned().collect(),
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
/// use hotpath_meta::future;
///
/// // Without logging (works with any output type)
/// let result = future!(async { NoDebugType::new() }).await;
///
/// // With logging (requires Debug on output type)
/// let result = future!(async { 42 }, log = true).await;
/// ```
#[macro_export]
macro_rules! future {
    // Basic: no Debug requirement
    ($fut:expr) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFuture::instrument_future($fut, FUTURE_LOC)
    }};

    // With logging: requires Debug
    ($fut:expr, log = true) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFutureLog::instrument_future_log($fut, FUTURE_LOC)
    }};
}
