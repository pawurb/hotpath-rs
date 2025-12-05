use crate::output;

#[doc(hidden)]
pub use cfg_if::cfg_if;
pub use hotpath_macros::{future_fn, main, measure, measure_all, skip};

pub mod channels;
pub mod futures;
pub mod streams;
#[cfg(feature = "threads")]
pub mod threads;

pub(crate) mod functions;

pub use channels::{InstrumentChannel, InstrumentChannelLog};
pub use futures::{InstrumentFuture, InstrumentFutureLog};
pub use streams::{InstrumentStream, InstrumentStreamLog};

pub(crate) const MAX_RESULT_LEN: usize = 1536;

pub use functions::guard::{FunctionsGuard, FunctionsGuardBuilder};

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        #[doc(hidden)]
        pub use tokio::runtime::{Handle, RuntimeFlavor};

        // Memory allocations profiling using a custom global allocator
        #[global_allocator]
        static GLOBAL: functions::alloc::allocator::CountingAllocator = functions::alloc::allocator::CountingAllocator {};

        pub use functions::alloc::guard::{MeasurementGuard, MeasurementGuardWithLog};
        pub use functions::alloc::state::FunctionStats;
    } else {
        // Time-based profiling (when no allocation features are enabled)
        pub use functions::timing::guard::{MeasurementGuard, MeasurementGuardWithLog};
        pub use functions::timing::state::FunctionStats;
    }
}

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

/// Measures the execution time or memory allocations of a code block.
///
/// This macro wraps a block of code with profiling instrumentation, similar to the
/// [`measure`](hotpath_macros::measure) attribute macro but for inline code blocks.
/// The block is labeled with a static string identifier.
///
/// # Arguments
///
/// * `$label` - A static string label to identify this code block in the profiling report
/// * `$expr` - The expression or code block to measure
///
/// # Behavior
///
/// The macro automatically uses the appropriate measurement based on enabled feature flags:
/// - **Time profiling** (default): Measures execution duration
/// - **Allocation profiling**: Tracks memory allocations when allocation features are enabled
///
/// # Examples
///
/// ```rust
/// # {
/// use std::time::Duration;
///
/// hotpath::measure_block!("data_processing", {
///     // Your code here
///     std::thread::sleep(Duration::from_millis(10));
/// });
/// # }
/// ```
///
/// # See Also
///
/// * [`measure`](hotpath_macros::measure) - Attribute macro for instrumenting functions
/// * [`main`](hotpath_macros::main) - Attribute macro that initializes profiling
#[macro_export]
macro_rules! measure_block {
    ($label:expr, $expr:expr) => {{
        let _guard = hotpath::MeasurementGuard::new($label, false, false);

        $expr
    }};
}

pub(crate) fn truncate_result(s: String) -> String {
    if s.len() <= MAX_RESULT_LEN {
        s
    } else {
        format!("{}...", &s[..MAX_RESULT_LEN.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_hotpath_is_send_sync() {
        is_send_sync::<FunctionsGuard>();
    }
}
