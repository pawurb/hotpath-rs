//! Function profiling module - measures execution time and memory allocations per function.

use std::{collections::HashMap, sync::OnceLock, sync::RwLock, time::Duration};

use arc_swap::ArcSwapOption;
use crossbeam_channel::{bounded, Sender};

use crate::{metrics_server::RECV_TIMEOUT_MS, FunctionLogsJson, FunctionsJson};

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        pub mod alloc;
        use alloc::state::FunctionsState;
        use tokio::runtime::{Handle, RuntimeFlavor};
        pub use alloc::guard::{MeasurementGuard, MeasurementGuardWithLog};
        pub use alloc::state::FunctionStats;
    } else {
        pub mod timing;
        use timing::state::FunctionsState;
        pub use timing::guard::{MeasurementGuard, MeasurementGuardWithLog};
        pub use timing::state::FunctionStats;
    }
}

pub(crate) use crate::output::truncate_result;

impl MeasurementGuard {
    pub fn build(measurement_name: &'static str, wrapper: bool, _is_async: bool) -> Self {
        #[allow(clippy::needless_bool)]
        let unsupported_async = if wrapper {
            // Top wrapper functions are not inside a runtime
            false
        } else {
            cfg_if::cfg_if! {
                if #[cfg(feature = "hotpath-alloc")] {
                    // For allocation profiling: mark async as unsupported unless
                    // running on Tokio CurrentThread. Non-Tokio runtimes are unsupported.
                    if _is_async {
                        match Handle::try_current() {
                            Ok(h) => h.runtime_flavor() != RuntimeFlavor::CurrentThread,
                            Err(_) => true,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };

        MeasurementGuard::new(measurement_name, wrapper, unsupported_async)
    }
}

impl MeasurementGuardWithLog {
    pub fn build(measurement_name: &'static str, wrapper: bool, _is_async: bool) -> Self {
        #[allow(clippy::needless_bool)]
        let unsupported_async = if wrapper {
            false
        } else {
            cfg_if::cfg_if! {
                if #[cfg(feature = "hotpath-alloc")] {
                    if _is_async {
                        match Handle::try_current() {
                            Ok(h) => h.runtime_flavor() != RuntimeFlavor::CurrentThread,
                            Err(_) => true,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };

        MeasurementGuardWithLog::new(measurement_name, wrapper, unsupported_async)
    }
}

/// Measure a sync function and log its return value.
#[doc(hidden)]
#[inline]
pub fn measure_with_log<T: std::fmt::Debug, F: FnOnce() -> T>(
    name: &'static str,
    wrapper: bool,
    is_async: bool,
    f: F,
) -> T {
    let guard = MeasurementGuardWithLog::build(name, wrapper, is_async);
    let result = f();
    guard.finish_with_result(&result);
    result
}

/// Measure an async function and log its return value.
#[doc(hidden)]
pub async fn measure_with_log_async<T: std::fmt::Debug, F, Fut>(name: &'static str, f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let guard = MeasurementGuardWithLog::build(name, false, true);
    let result = f().await;
    guard.finish_with_result(&result);
    result
}

pub(crate) static FUNCTIONS_STATE: OnceLock<ArcSwapOption<RwLock<FunctionsState>>> =
    OnceLock::new();

pub mod guard;

/// Query request sent from TUI HTTP server to profiler worker thread
pub(crate) enum FunctionsQuery {
    /// Request timing metrics snapshot
    Timing(Sender<FunctionsJson>),
    /// Request full metrics snapshot (allocation metrics) - returns None if hotpath-alloc not enabled
    Alloc(Sender<Option<FunctionsJson>>),
    /// Request timing function logs for a specific function (returns None if function not found)
    LogsTiming {
        function_name: String,
        response_tx: Sender<Option<FunctionLogsJson>>,
    },
    /// Request allocation function logs for a specific function (returns None if hotpath-alloc not enabled or function not found)
    LogsAlloc {
        function_name: String,
        response_tx: Sender<Option<FunctionLogsJson>>,
    },
}

/// Helper to send a query to the functions worker and receive the response.
fn query_functions_state<T, F>(make_query: F) -> Option<T>
where
    F: FnOnce(Sender<T>) -> FunctionsQuery,
{
    let arc_swap = FUNCTIONS_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();
    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<T>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx.send(make_query(response_tx)).ok()?;
        drop(state_guard);
        response_rx
            .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
            .ok()
    } else {
        None
    }
}

// Get instrumented functions profiling information
pub(crate) fn get_functions_timing_json() -> FunctionsJson {
    if let Some(metrics) = try_get_functions_timing_from_worker() {
        return metrics;
    }

    // Fallback if query fails: return empty functions data
    FunctionsJson {
        hotpath_profiling_mode: crate::output::ProfilingMode::Timing,
        total_elapsed: 0,
        description: "No timing data available yet".to_string(),
        caller_name: "hotpath".to_string(),
        percentiles: vec![95],
        data: crate::output::FunctionsDataJson(HashMap::new()),
    }
}

// Get instrumented functions calls information
pub(crate) fn get_function_logs_timing(function_name: &str) -> Option<FunctionLogsJson> {
    let name = function_name.to_string();
    query_functions_state(|response_tx| FunctionsQuery::LogsTiming {
        function_name: name,
        response_tx,
    })
    .flatten()
}

fn try_get_functions_timing_from_worker() -> Option<FunctionsJson> {
    query_functions_state(FunctionsQuery::Timing)
}

// Get a JSON representation of all functions and their allocations
// Will return None unless hotpath-alloc is enabled
pub(crate) fn get_functions_alloc_json() -> Option<FunctionsJson> {
    query_functions_state(FunctionsQuery::Alloc).flatten()
}

// Get instrumented function calls information
// Will return None unless hotpath-alloc is enabled
pub(crate) fn get_function_logs_alloc(function_name: &str) -> Option<FunctionLogsJson> {
    let name = function_name.to_string();
    query_functions_state(|response_tx| FunctionsQuery::LogsAlloc {
        function_name: name,
        response_tx,
    })
    .flatten()
}
