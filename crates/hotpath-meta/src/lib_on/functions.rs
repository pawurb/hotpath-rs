//! Function profiling module - measures execution time and memory allocations per function.

use std::{sync::OnceLock, sync::RwLock, time::Duration};

use arc_swap::ArcSwapOption;
use crossbeam_channel::{bounded, Sender};

use crate::json::JsonFunctionsList;
use crate::lib_on::START_TIME;
use crate::metrics_server::RECV_TIMEOUT_MS;
use crate::FunctionLogsList;

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc-meta")] {
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

#[inline]
pub(crate) fn is_exclude_wrapper_enabled() -> bool {
    std::env::var("HOTPATH_META_EXCLUDE_WRAPPER")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

impl MeasurementGuard {
    pub fn build(measurement_name: &'static str, wrapper: bool, _is_async: bool) -> Self {
        #[allow(clippy::needless_bool)]
        let unsupported_async = if wrapper {
            // Top wrapper functions are not inside a runtime
            false
        } else {
            cfg_if::cfg_if! {
                if #[cfg(feature = "hotpath-alloc-meta")] {
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
                if #[cfg(feature = "hotpath-alloc-meta")] {
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

/// Query request sent from TUI HTTP server to profiler worker thread
pub(crate) enum FunctionsQuery {
    /// Request timing metrics snapshot
    Timing(Sender<JsonFunctionsList>),
    /// Request full metrics snapshot (allocation metrics) - returns None if hotpath-alloc-meta not enabled
    Alloc(Sender<Option<JsonFunctionsList>>),
    /// Request timing function logs for a specific function (returns None if function not found)
    LogsTiming {
        function_name: String,
        response_tx: Sender<Option<FunctionLogsList>>,
    },
    /// Request allocation function logs for a specific function (returns None if hotpath-alloc-meta not enabled or function not found)
    LogsAlloc {
        function_name: String,
        response_tx: Sender<Option<FunctionLogsList>>,
    },
}

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

pub(crate) fn get_functions_timing_json() -> JsonFunctionsList {
    if let Some(formatted) = query_functions_state(FunctionsQuery::Timing) {
        return formatted;
    }

    JsonFunctionsList::empty_fallback(get_current_elapsed_ns())
}

pub(crate) fn get_function_logs_timing(function_name: &str) -> Option<FunctionLogsList> {
    let name = function_name.to_string();
    query_functions_state(|response_tx| FunctionsQuery::LogsTiming {
        function_name: name,
        response_tx,
    })
    .flatten()
}

pub(crate) fn get_functions_alloc_json() -> Option<JsonFunctionsList> {
    query_functions_state(FunctionsQuery::Alloc).flatten()
}

// Get instrumented function calls information
// Will return None unless hotpath-alloc-meta is enabled
pub(crate) fn get_function_logs_alloc(function_name: &str) -> Option<FunctionLogsList> {
    let name = function_name.to_string();
    query_functions_state(|response_tx| FunctionsQuery::LogsAlloc {
        function_name: name,
        response_tx,
    })
    .flatten()
}
