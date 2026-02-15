//! Function profiling module - measures execution time and memory allocations per function.

use std::sync::atomic::{AtomicU64, Ordering};
use std::{sync::LazyLock, sync::OnceLock, sync::RwLock, time::Duration};

use arc_swap::ArcSwapOption;
use crossbeam_channel::{bounded, Sender};

use crate::json::JsonFunctionsList;
use crate::lib_on::START_TIME;
use crate::metrics_server::RECV_TIMEOUT_MS;
use crate::FunctionLogsList;

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

pub(crate) static FUNCTIONS_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_function_id() -> u64 {
    FUNCTIONS_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub(crate) static EXCLUDE_WRAPPER: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_EXCLUDE_WRAPPER")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
});

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
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
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::future_fn(log = true))]
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
#[derive(Debug)]
pub(crate) enum FunctionsQuery {
    /// Request timing metrics snapshot
    Timing(Sender<JsonFunctionsList>),
    /// Request full metrics snapshot (allocation metrics) - returns None if hotpath-alloc not enabled
    Alloc(Sender<Option<JsonFunctionsList>>),
    /// Request timing function logs for a specific function by ID
    LogsTiming {
        function_id: u64,
        response_tx: Sender<Option<FunctionLogsList>>,
    },
    /// Request allocation function logs for a specific function by ID
    LogsAlloc {
        function_id: u64,
        response_tx: Sender<Option<FunctionLogsList>>,
    },
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn get_current_elapsed_ns() -> u64 {
    START_TIME
        .get()
        .map(|start| start.elapsed().as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_functions_timing_json() -> JsonFunctionsList {
    if let Some(formatted) = query_functions_state(FunctionsQuery::Timing) {
        return formatted;
    }

    JsonFunctionsList::empty_fallback(get_current_elapsed_ns())
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_function_logs_timing(function_id: u64) -> Option<FunctionLogsList> {
    query_functions_state(|response_tx| FunctionsQuery::LogsTiming {
        function_id,
        response_tx,
    })
    .flatten()
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_functions_alloc_json() -> Option<JsonFunctionsList> {
    query_functions_state(FunctionsQuery::Alloc).flatten()
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_function_logs_alloc(function_id: u64) -> Option<FunctionLogsList> {
    query_functions_state(|response_tx| FunctionsQuery::LogsAlloc {
        function_id,
        response_tx,
    })
    .flatten()
}
