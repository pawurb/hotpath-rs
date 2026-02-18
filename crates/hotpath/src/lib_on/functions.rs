//! Function profiling module - measures execution time and memory allocations per function.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, OnceLock};

use crate::json::JsonFunctionsList;
use crate::lib_on::START_TIME;
use crate::output::MetricsProvider;
use crate::FunctionLogsList;

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        pub mod alloc;
        use alloc::state::FunctionsState;
        pub use alloc::guard::{MeasurementGuard, MeasurementGuardWithLog};
        pub use alloc::state::FunctionStats;
    } else {
        pub mod timing;
        use timing::state::FunctionsState;
        pub use timing::guard::{MeasurementGuard, MeasurementGuardWithLog};
        pub use timing::state::FunctionStats;
    }
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
    let val = std::env::var("HOTPATH_FOCUS").ok()?;
    if let Some(pattern) = val.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
        Some(Focus::Regex(
            regex::Regex::new(pattern).expect("Invalid HOTPATH_FOCUS regex pattern"),
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

pub(crate) static EXCLUDE_WRAPPER: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_EXCLUDE_WRAPPER")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
});

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl MeasurementGuard {
    pub fn build(measurement_name: &'static str, wrapper: bool, is_async: bool) -> Self {
        let skipped = !wrapper && !is_focused(measurement_name);
        MeasurementGuard::new(measurement_name, wrapper, skipped, is_async)
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl MeasurementGuardWithLog {
    pub fn build(measurement_name: &'static str, wrapper: bool, is_async: bool) -> Self {
        let skipped = !wrapper && !is_focused(measurement_name);
        MeasurementGuardWithLog::new(measurement_name, wrapper, skipped, is_async)
    }
}

/// Measure a sync function and log its return value.
#[doc(hidden)]
#[inline]
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn measure_with_log<T: std::fmt::Debug, F: FnOnce() -> T>(
    name: &'static str,
    wrapper: bool,
    f: F,
) -> T {
    let guard = MeasurementGuardWithLog::build(name, wrapper, false);
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

pub(crate) static FUNCTIONS_STATE: OnceLock<FunctionsState> = OnceLock::new();

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn get_current_elapsed_ns() -> u64 {
    START_TIME
        .get()
        .map(|start| start.elapsed().as_nanos() as u64)
        .unwrap_or(0)
}

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        use alloc::report::{StatsData, TimingStatsData};
        use alloc::state::{build_alloc_function_logs, build_alloc_timing_logs};
    } else {
        use timing::report::StatsData;
        use timing::state::build_timing_function_logs;
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_functions_timing_json() -> JsonFunctionsList {
    let Some(state) = FUNCTIONS_STATE.get() else {
        return JsonFunctionsList::empty_fallback(get_current_elapsed_ns());
    };
    let Ok(stats) = state.stats_map.read() else {
        return JsonFunctionsList::empty_fallback(get_current_elapsed_ns());
    };
    let total_elapsed = state.start_time.elapsed();
    let current_elapsed_ns = total_elapsed.as_nanos() as u64;

    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc")] {
            let provider = TimingStatsData::new(
                &stats,
                total_elapsed,
                state.percentiles.clone(),
                state.caller_name,
                state.limit,
            );
        } else {
            let provider = StatsData::new(
                &stats,
                total_elapsed,
                state.percentiles.clone(),
                state.caller_name,
                state.limit,
            );
        }
    }

    JsonFunctionsList::from_provider(&provider, current_elapsed_ns)
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_function_logs_timing(function_id: u32) -> Option<FunctionLogsList> {
    let state = FUNCTIONS_STATE.get()?;
    let stats = state.stats_map.read().ok()?;

    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc")] {
            build_alloc_timing_logs(&stats, function_id)
        } else {
            build_timing_function_logs(&stats, function_id)
        }
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_functions_alloc_json() -> Option<JsonFunctionsList> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc")] {
            let state = FUNCTIONS_STATE.get()?;
            let stats = state.stats_map.read().ok()?;
            let total_elapsed = state.start_time.elapsed();
            let current_elapsed_ns = total_elapsed.as_nanos() as u64;
            let provider = StatsData::new(
                &stats,
                total_elapsed,
                state.percentiles.clone(),
                state.caller_name,
                state.limit,
            );
            Some(JsonFunctionsList::from_provider(&provider, current_elapsed_ns))
        } else {
            None
        }
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_function_logs_alloc(function_id: u32) -> Option<FunctionLogsList> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-alloc")] {
            let state = FUNCTIONS_STATE.get()?;
            let stats = state.stats_map.read().ok()?;
            build_alloc_function_logs(&stats, function_id)
        } else {
            let _ = function_id;
            None
        }
    }
}
