//! Futures instrumentation module - tracks async Future lifecycle and poll statistics.

use crate::channels::{get_log_limit, resolve_label, START_TIME};
use crate::http_server::HTTP_SERVER_PORT;
use crossbeam_channel::{unbounded, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod guard;
pub(crate) mod wrapper;

pub use guard::{FuturesGuard, FuturesGuardBuilder};
pub use wrapper::{InstrumentedFuture, InstrumentedFutureLog};

// Re-export Format from crate root
pub use crate::Format;
// Re-export JSON types from json module
pub use crate::json::{FutureCall, FutureCalls, FutureState, FuturesJson, SerializableFutureStats};

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

    // First try read lock
    {
        let read_guard = map.read().unwrap();
        if let Some(&future_id) = read_guard.get(source) {
            return (future_id, false);
        }
    }

    // Need write lock to insert
    let mut write_guard = map.write().unwrap();
    // Double-check after acquiring write lock
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

/// Query types for requesting data from the futures worker thread.
pub(crate) enum FutureQuery {
    GetAllStats(CbSender<FuturesJson>),
    GetCalls {
        future_id: u64,
        response_tx: CbSender<Option<FutureCalls>>,
    },
}

pub(crate) type FuturesStatsState = (CbSender<FutureEvent>, CbSender<FutureQuery>);

static FUTURES_STATE: OnceLock<FuturesStatsState> = OnceLock::new();

/// Initialize the futures event collection system (called on first instrumented future).
#[doc(hidden)]
pub fn init_futures_state() {
    FUTURES_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        crate::http_server::start_metrics_server_once(*HTTP_SERVER_PORT);

        let (event_tx, event_rx) = unbounded::<FutureEvent>();
        let (query_tx, query_rx) = unbounded::<FutureQuery>();

        std::thread::Builder::new()
            .name("hp-futures".into())
            .spawn(move || {
                let mut stats_map = HashMap::<u64, FutureStats>::new();

                loop {
                    crossbeam_channel::select! {
                        recv(event_rx) -> event => {
                            match event {
                                Ok(event) => process_future_event(&mut stats_map, event),
                                Err(_) => break,
                            }
                        }
                        recv(query_rx) -> query => {
                            match query {
                                Ok(FutureQuery::GetAllStats(response_tx)) => {
                                    let json = build_futures_json(&stats_map);
                                    let _ = response_tx.send(json);
                                }
                                Ok(FutureQuery::GetCalls { future_id, response_tx }) => {
                                    let calls = stats_map.get(&future_id).map(|s| FutureCalls {
                                        id: future_id.to_string(),
                                        calls: s.calls.iter().rev().cloned().collect(),
                                    });
                                    let _ = response_tx.send(calls);
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            })
            .expect("Failed to spawn futures event collector thread");

        (event_tx, query_tx)
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

/// Compare two serializable future stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source).
fn compare_serializable_stats(
    a: &SerializableFutureStats,
    b: &SerializableFutureStats,
) -> std::cmp::Ordering {
    match (a.has_custom_label, b.has_custom_label) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a.label.cmp(&b.label),
        (false, false) => a.source.cmp(&b.source),
    }
}

/// Build FuturesJson from the stats map (called on worker thread).
fn build_futures_json(stats_map: &HashMap<u64, FutureStats>) -> FuturesJson {
    let mut futures: Vec<SerializableFutureStats> = stats_map
        .values()
        .map(SerializableFutureStats::from)
        .collect();
    futures.sort_by(compare_serializable_stats);

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    FuturesJson {
        current_elapsed_ns,
        futures,
    }
}

pub fn get_futures_json() -> FuturesJson {
    if let Some((_, query_tx)) = FUTURES_STATE.get() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        if query_tx.send(FutureQuery::GetAllStats(tx)).is_ok() {
            if let Ok(json) = rx.recv_timeout(Duration::from_millis(250)) {
                return json;
            }
        }
    }
    // Return empty on timeout/error
    FuturesJson {
        current_elapsed_ns: 0,
        futures: vec![],
    }
}

pub fn get_future_calls(future_id: u64) -> Option<FutureCalls> {
    if let Some((_, query_tx)) = FUTURES_STATE.get() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        if query_tx
            .send(FutureQuery::GetCalls {
                future_id,
                response_tx: tx,
            })
            .is_ok()
        {
            return rx.recv_timeout(Duration::from_millis(250)).ok().flatten();
        }
    }
    None
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
