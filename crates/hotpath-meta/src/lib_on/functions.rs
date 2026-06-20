//! Function profiling module - measures execution time and memory allocations per function.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::{sync::LazyLock, sync::OnceLock, sync::RwLock, time::Duration};

use crossbeam_channel::{bounded, Sender};

use crate::json::JsonFunctionsList;
use crate::lib_on::START_TIME;
use crate::metrics_server::RECV_TIMEOUT_MS;
use crate::output::FunctionLogsList;

#[cfg(feature = "hotpath-cpu-meta")]
pub(crate) mod cpu;

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc-meta")] {
        pub(crate) mod alloc;
        use alloc::state::{FunctionsState, Measurement};
        pub(crate) use alloc::guard::AsyncAllocBridge;
        pub use alloc::guard::{MeasurementGuardAsync, MeasurementGuardSync};
        pub(crate) use alloc::guard::{MeasurementGuardAsyncWithLog, MeasurementGuardSyncWithLog};
    } else {
        pub(crate) mod timing;
        use timing::state::{FunctionsState, Measurement};
        #[derive(Default)]
        pub(crate) struct AsyncAllocBridge;
        impl AsyncAllocBridge {
            #[inline]
            pub(crate) fn add(&self, _bytes: u64, _count: u64) {}
        }
        pub use timing::guard::MeasurementGuard as MeasurementGuardAsync;
        pub use timing::guard::MeasurementGuard as MeasurementGuardSync;
        pub(crate) use timing::guard::MeasurementGuardWithLog as MeasurementGuardAsyncWithLog;
        pub(crate) use timing::guard::MeasurementGuardWithLog as MeasurementGuardSyncWithLog;
    }
}

pub(crate) struct FunctionStatsConfig {
    pub(crate) total_elapsed: Duration,
    pub(crate) percentiles: Vec<f64>,
    pub(crate) caller_name: &'static str,
    pub(crate) limit: usize,
}

pub(crate) static FUNCTIONS_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub(crate) fn next_function_id() -> u32 {
    FUNCTIONS_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

enum Focus {
    Text(String),
    Regex(regex::Regex),
}

static FOCUS_FILTER: LazyLock<Option<Focus>> = LazyLock::new(|| {
    let val = std::env::var("HOTPATH_META_FOCUS").ok()?;
    if let Some(pattern) = val.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
        Some(Focus::Regex(
            regex::Regex::new(pattern).expect("Invalid HOTPATH_META_FOCUS regex pattern"),
        ))
    } else {
        Some(Focus::Text(val))
    }
});

#[inline]
fn is_focused(name: &str) -> bool {
    match &*FOCUS_FILTER {
        None => true,
        Some(Focus::Text(filter)) => name.contains(filter.as_str()),
        Some(Focus::Regex(re)) => re.is_match(name),
    }
}

pub(crate) static EXCLUDE_WRAPPER: LazyLock<bool> =
    LazyLock::new(|| crate::shared::env_flag("HOTPATH_META_EXCLUDE_WRAPPER"));

#[doc(hidden)]
pub fn build_measurement_guard_sync(
    measurement_name: &'static str,
    wrapper: bool,
) -> MeasurementGuardSync {
    let skipped = !wrapper && !is_focused(measurement_name);
    MeasurementGuardSync::new(measurement_name, wrapper, skipped)
}

#[doc(hidden)]
fn build_measurement_guard_async(
    measurement_name: &'static str,
    wrapper: bool,
) -> MeasurementGuardAsync {
    let skipped = !wrapper && !is_focused(measurement_name);
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc-meta")] {
            MeasurementGuardAsync::new(measurement_name, wrapper, skipped, None)
        } else {
            MeasurementGuardAsync::new(measurement_name, wrapper, skipped)
        }
    }
}

#[inline]
fn make_alloc_bridge(skipped: bool) -> Option<std::sync::Arc<AsyncAllocBridge>> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc-meta")] {
            if skipped { None } else { Some(std::sync::Arc::new(AsyncAllocBridge::default())) }
        } else {
            let _ = skipped;
            None
        }
    }
}

#[inline]
fn build_measurement_guard_async_with_bridge(
    measurement_name: &'static str,
    wrapper: bool,
) -> (
    MeasurementGuardAsync,
    Option<std::sync::Arc<AsyncAllocBridge>>,
) {
    let skipped = !wrapper && !is_focused(measurement_name);
    let alloc_bridge = make_alloc_bridge(skipped);

    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc-meta")] {
            let guard = MeasurementGuardAsync::new(
                measurement_name,
                wrapper,
                skipped,
                alloc_bridge.clone(),
            );
            (guard, alloc_bridge)
        } else {
            let guard = MeasurementGuardAsync::new(measurement_name, wrapper, skipped);
            (guard, alloc_bridge)
        }
    }
}

fn build_measurement_guard_sync_with_log(
    measurement_name: &'static str,
    wrapper: bool,
) -> MeasurementGuardSyncWithLog {
    let skipped = !wrapper && !is_focused(measurement_name);
    MeasurementGuardSyncWithLog::new(measurement_name, wrapper, skipped)
}

#[cfg(not(feature = "hotpath-alloc-meta"))]
fn build_measurement_guard_async_with_log(
    measurement_name: &'static str,
    wrapper: bool,
) -> MeasurementGuardAsyncWithLog {
    let skipped = !wrapper && !is_focused(measurement_name);
    MeasurementGuardAsyncWithLog::new(measurement_name, wrapper, skipped)
}

#[inline]
fn build_measurement_guard_async_with_log_bridge(
    measurement_name: &'static str,
    wrapper: bool,
) -> (
    MeasurementGuardAsyncWithLog,
    Option<std::sync::Arc<AsyncAllocBridge>>,
) {
    let skipped = !wrapper && !is_focused(measurement_name);
    let alloc_bridge = make_alloc_bridge(skipped);

    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc-meta")] {
            let guard = MeasurementGuardAsyncWithLog::new(
                measurement_name,
                wrapper,
                skipped,
                alloc_bridge.clone(),
            );
            (guard, alloc_bridge)
        } else {
            let guard = MeasurementGuardAsyncWithLog::new(measurement_name, wrapper, skipped);
            (guard, alloc_bridge)
        }
    }
}

#[doc(hidden)]
#[inline]
pub fn measure_sync<T, F: FnOnce() -> T>(measurement_loc: &'static str, f: F) -> T {
    let _guard = build_measurement_guard_sync(measurement_loc, false);
    f()
}

#[doc(hidden)]
#[inline]
pub fn measure_sync_log<T: std::fmt::Debug, F: FnOnce() -> T>(
    measurement_loc: &'static str,
    f: F,
) -> T {
    let guard = build_measurement_guard_sync_with_log(measurement_loc, false);
    let result = f();
    guard.finish_with_result(&result);
    result
}

#[doc(hidden)]
pub async fn measure_async_log<T: std::fmt::Debug, Fut>(
    measurement_loc: &'static str,
    fut: Fut,
) -> T
where
    Fut: Future<Output = T>,
{
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc-meta")] {
            let (guard, alloc_bridge) = build_measurement_guard_async_with_log_bridge(measurement_loc, false);
            let result = crate::futures::wrapper::InstrumentedFuture::new(
                fut,
                measurement_loc,
                None,
                alloc_bridge,
                false,
            )
            .await;
            guard.finish_with_result(&result);
            result
        } else {
            let guard = build_measurement_guard_async_with_log(measurement_loc, false);
            let result = fut.await;
            guard.finish_with_result(&result);
            result
        }
    }
}

#[doc(hidden)]
pub async fn measure_async<T, Fut>(measurement_loc: &'static str, fut: Fut) -> T
where
    Fut: Future<Output = T>,
{
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc-meta")] {
            let (_guard, alloc_bridge) =
                build_measurement_guard_async_with_bridge(measurement_loc, false);
            crate::futures::wrapper::InstrumentedFuture::new(
                fut,
                measurement_loc,
                None,
                alloc_bridge,
                false,
            )
            .await
        } else {
            let _guard = build_measurement_guard_async(measurement_loc, false);
            fut.await
        }
    }
}

#[doc(hidden)]
pub async fn measure_async_future<T, Fut>(measurement_loc: &'static str, fut: Fut) -> T
where
    Fut: Future<Output = T>,
{
    crate::futures::init_futures_state();

    let (_guard, alloc_bridge) = build_measurement_guard_async_with_bridge(measurement_loc, false);
    crate::futures::wrapper::InstrumentedFuture::new(fut, measurement_loc, None, alloc_bridge, true)
        .await
}

#[doc(hidden)]
pub async fn measure_async_future_log<T, Fut>(measurement_loc: &'static str, fut: Fut) -> T
where
    T: std::fmt::Debug,
    Fut: Future<Output = T>,
{
    crate::futures::init_futures_state();

    let (guard, alloc_bridge) =
        build_measurement_guard_async_with_log_bridge(measurement_loc, false);
    let result = crate::futures::wrapper::InstrumentedFutureLog::new(
        fut,
        measurement_loc,
        None,
        alloc_bridge,
        true,
    )
    .await;
    guard.finish_with_result(&result);
    result
}

pub(crate) static FUNCTIONS_STATE: OnceLock<Arc<RwLock<FunctionsState>>> = OnceLock::new();

pub(crate) static FUNCTIONS_QUERY_TX: OnceLock<WorkerTx> = OnceLock::new();

static CPU_LABEL_ALIASES: OnceLock<RwLock<HashMap<&'static str, &'static str>>> = OnceLock::new();

#[doc(hidden)]
pub fn register_cpu_label_alias(label: &'static str, symbol: &'static str) {
    let map = CPU_LABEL_ALIASES.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(mut w) = map.write() {
        w.entry(label).or_insert(symbol);
    }
}

#[cfg(feature = "hotpath-cpu-meta")]
pub(crate) fn get_cpu_label_aliases() -> HashMap<&'static str, &'static str> {
    CPU_LABEL_ALIASES
        .get()
        .and_then(|m| m.read().ok().map(|g| g.clone()))
        .unwrap_or_default()
}

/// Query request sent from TUI HTTP server to profiler worker thread
#[derive(Debug)]
pub(crate) enum FunctionsQuery {
    /// Request timing metrics snapshot
    Timing(Sender<JsonFunctionsList>),
    /// Request full metrics snapshot (allocation metrics) - returns None if hotpath-alloc-meta not enabled
    Alloc(Sender<Option<JsonFunctionsList>>),
    /// Request the names + worker-assigned ids of functions that have been registered
    #[cfg(feature = "hotpath-cpu-meta")]
    NamesAndIds(Sender<HashMap<&'static str, u32>>),
    /// Request timing function logs for a specific function by ID
    LogsTiming {
        function_id: u32,
        response_tx: Sender<Option<FunctionLogsList>>,
    },
    /// Request allocation function logs for a specific function by ID
    LogsAlloc {
        function_id: u32,
        response_tx: Sender<Option<FunctionLogsList>>,
    },
}

/// Everything the functions worker consumes, carried on a single channel so the
/// channel can be instrumented with `channel!(..., wrap = true)` under
/// `hotpath-meta` (crossbeam `select!` is incompatible with wrapper endpoints).
///
/// `Query`/`Shutdown` queue FIFO behind pending `Measurements`. The worker drains a
/// 64-measurement batch in a few µs, so while it keeps up (the normal case) a query
/// waits well under a millisecond, and a transient backlog clears in a few ms. The
/// wait only grows unbounded once the worker is saturated - when the unbounded
/// measurement queue is already the larger problem.
#[derive(Debug)]
pub(crate) enum WorkerMsg {
    Measurements(Vec<Measurement>),
    Query(FunctionsQuery),
    Shutdown,
}

pub(crate) type WorkerTx = crossbeam_channel::Sender<WorkerMsg>;

fn get_current_elapsed_ns() -> u64 {
    START_TIME
        .get()
        .map(|start| start.elapsed().as_nanos() as u64)
        .unwrap_or(0)
}

fn query_functions_state<T, F>(make_query: F) -> Option<T>
where
    F: FnOnce(Sender<T>) -> FunctionsQuery,
{
    let query_tx = FUNCTIONS_QUERY_TX.get()?;
    let (response_tx, response_rx) = bounded::<T>(1);
    query_tx
        .send(WorkerMsg::Query(make_query(response_tx)))
        .ok()?;
    response_rx
        .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
        .ok()
}

pub(crate) fn get_functions_timing_json() -> JsonFunctionsList {
    if let Some(formatted) = query_functions_state(FunctionsQuery::Timing) {
        return formatted;
    }

    JsonFunctionsList::empty_fallback(get_current_elapsed_ns())
}

pub(crate) fn get_function_logs_timing(function_id: u32) -> Option<FunctionLogsList> {
    query_functions_state(|response_tx| FunctionsQuery::LogsTiming {
        function_id,
        response_tx,
    })
    .flatten()
}

pub(crate) fn get_functions_alloc_json() -> Option<JsonFunctionsList> {
    query_functions_state(FunctionsQuery::Alloc).flatten()
}

#[cfg(feature = "hotpath-cpu-meta")]
pub(crate) fn get_instrumented_names_and_ids() -> Option<HashMap<&'static str, u32>> {
    query_functions_state(FunctionsQuery::NamesAndIds)
}

pub(crate) fn get_function_logs_alloc(function_id: u32) -> Option<FunctionLogsList> {
    query_functions_state(|response_tx| FunctionsQuery::LogsAlloc {
        function_id,
        response_tx,
    })
    .flatten()
}
