#[doc(hidden)]
pub use cfg_if::cfg_if;
pub use hotpath_macros::{future_fn, main, measure, measure_all, skip};

pub mod channels;
pub mod futures;
pub mod streams;
#[cfg(feature = "threads")]
pub mod threads;

pub mod functions;

pub use channels::{InstrumentChannel, InstrumentChannelLog};
pub use futures::{InstrumentFuture, InstrumentFutureLog};
pub use streams::{InstrumentStream, InstrumentStreamLog};

pub use functions::guard::{FunctionsGuard, FunctionsGuardBuilder};
pub use functions::{
    measure_with_log, measure_with_log_async, FunctionStats, MeasurementGuard,
    MeasurementGuardWithLog,
};

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        #[doc(hidden)]
        pub use tokio::runtime::{Handle, RuntimeFlavor};

        // Memory allocations profiling using a custom global allocator
        #[global_allocator]
        static GLOBAL: functions::alloc::allocator::CountingAllocator = functions::alloc::allocator::CountingAllocator {};
    }
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
        let _guard = hotpath::functions::MeasurementGuard::new($label, false, false);

        $expr
    }};
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
