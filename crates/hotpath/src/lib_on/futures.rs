//! Futures instrumentation module - tracks async Future lifecycle and poll statistics.

use crate::channels::{get_log_limit, resolve_label, START_TIME};
use crate::metrics_server::METRICS_SERVER_PORT;
use crossbeam_channel::{unbounded, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod guard;
pub(crate) mod wrapper;

pub use guard::{FuturesGuard, FuturesGuardBuilder};
pub use wrapper::{InstrumentedFuture, InstrumentedFutureLog};

pub use crate::json::{FutureCall, FutureCalls, FutureState, FuturesJson, SerializableFutureStats};
pub use crate::Format;

pub(crate) static FUTURE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
pub(crate) static FUTURE_CALL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    let future_id = FUTURE_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    write_guard.insert(source, future_id);
    (future_id, true)
}

/// Aggregated statistics for a source location.
#[derive(Debug, Clone)]
pub struct FutureStats {
    pub id: u64,
    pub source: &'static str,
    pub label: Option<String>,
    pub calls: VecDeque<FutureCall>,
    pub call_count: u64,
}

impl FutureStats {
    fn new(id: u64, source: &'static str, label: Option<String>) -> Self {
        Self {
            id,
            source,
            label,
            calls: VecDeque::new(),
            call_count: 0,
        }
    }

    /// Total polls across all invocations
    pub fn total_polls(&self) -> u64 {
        self.calls.iter().map(|c| c.poll_count).sum()
    }

    /// Find a call by ID
    fn find_call_mut(&mut self, id: u64) -> Option<&mut FutureCall> {
        self.calls.iter_mut().find(|c| c.id == id)
    }
}

impl From<&FutureStats> for SerializableFutureStats {
    fn from(future_stats: &FutureStats) -> Self {
        let label = resolve_label(future_stats.source, future_stats.label.as_deref(), None);

        Self {
            id: future_stats.id,
            source: future_stats.source.to_string(),
            label,
            has_custom_label: future_stats.label.is_some(),
            call_count: future_stats.call_count,
            total_polls: future_stats.total_polls(),
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

/// State type: event sender + shared stats map
pub(crate) type FuturesStatsState = (
    CbSender<FutureEvent>,
    Arc<RwLock<HashMap<u64, FutureStats>>>,
);

static FUTURES_STATE: OnceLock<FuturesStatsState> = OnceLock::new();

/// Initialize the futures event collection system (called on first instrumented future).
#[doc(hidden)]
pub fn init_futures_state() {
    FUTURES_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        let (event_tx, event_rx) = unbounded::<FutureEvent>();
        let stats_map = Arc::new(RwLock::new(HashMap::<u64, FutureStats>::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-futures".into())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    let mut stats = stats_map_clone.write().unwrap();
                    process_future_event(&mut stats, event);
                }
            })
            .expect("Failed to spawn futures event collector thread");

        (event_tx, stats_map)
    });
}

/// Process a future event and update stats.
fn process_future_event(stats_map: &mut HashMap<u64, FutureStats>, event: FutureEvent) {
    match event {
        FutureEvent::Created {
            future_id,
            source,
            display_label,
        } => {
            stats_map.insert(
                future_id,
                FutureStats::new(future_id, source, display_label),
            );
        }
        FutureEvent::CallCreated { future_id, call_id } => {
            if let Some(future_stats) = stats_map.get_mut(&future_id) {
                future_stats.call_count += 1;
                let limit = get_log_limit();
                if future_stats.calls.len() >= limit {
                    future_stats.calls.pop_front();
                }
                future_stats
                    .calls
                    .push_back(FutureCall::new(call_id, future_id));
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
    if let Some((tx, _)) = FUTURES_STATE.get() {
        let _ = tx.send(event);
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
fn compare_future_stats(a: &FutureStats, b: &FutureStats) -> std::cmp::Ordering {
    let a_has_label = a.label.is_some();
    let b_has_label = b.label.is_some();

    match (a_has_label, b_has_label) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a.label.as_ref().unwrap().cmp(b.label.as_ref().unwrap()),
        (false, false) => a.source.cmp(b.source),
    }
}

fn get_all_future_stats() -> HashMap<u64, FutureStats> {
    if let Some((_, stats_map)) = FUTURES_STATE.get() {
        stats_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

pub(crate) fn get_sorted_future_stats() -> Vec<FutureStats> {
    let mut stats: Vec<FutureStats> = get_all_future_stats().into_values().collect();
    stats.sort_by(compare_future_stats);
    stats
}

pub fn get_futures_json() -> FuturesJson {
    let futures = get_sorted_future_stats()
        .iter()
        .map(SerializableFutureStats::from)
        .collect();

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    FuturesJson {
        current_elapsed_ns,
        futures,
    }
}

pub fn get_future_calls(future_id: u64) -> Option<FutureCalls> {
    let stats = get_all_future_stats();
    stats.get(&future_id).map(|s| FutureCalls {
        id: future_id.to_string(),
        calls: s.calls.iter().rev().cloned().collect(),
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
