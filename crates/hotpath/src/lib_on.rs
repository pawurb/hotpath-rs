#[doc(hidden)]
pub use cfg_if::cfg_if;
pub use hotpath_macros::{future_fn, main, measure, measure_all, skip};

use std::sync::OnceLock;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub(crate) static START_TIME: OnceLock<Instant> = OnceLock::new();

pub mod channels;
pub mod data_flow;
pub mod debug;
pub mod futures;
pub mod streams;
#[cfg(feature = "threads")]
pub mod threads;
#[cfg(feature = "tokio")]
pub mod tokio_runtime;

pub mod functions;

pub use channels::{InstrumentChannel, InstrumentChannelLog};
pub use futures::{InstrumentFuture, InstrumentFutureLog};
pub use streams::{InstrumentStream, InstrumentStreamLog};

pub mod hotpath_guard;
pub(crate) mod report;

pub use functions::{
    measure_with_log, measure_with_log_async, FunctionStats, MeasurementGuard,
    MeasurementGuardWithLog,
};
pub use hotpath_guard::{HotpathGuard, HotpathGuardBuilder};

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
        let _guard = hotpath::functions::MeasurementGuard::build($label, false, false);

        $expr
    }};
}

/// Debug macro that tracks debug output in the profiler.
///
/// Works like `std::dbg!` but sends debug logs to a background worker thread
/// for tracking in the profiler. The logs can be viewed in the TUI or via
/// the HTTP API at `/debug` and `/debug/{id}/logs`.
///
/// # Variants
///
/// - `dbg!(expr)` - Returns value, logs expression + result
/// - `dbg!(a, b, c)` - Multiple expressions, returns tuple
///
/// # Examples
///
/// ```rust,ignore
/// use hotpath::dbg;
///
/// // Debug a single value
/// let x = dbg!(1 + 2);  // returns 3, logs "1 + 2 = 3"
///
/// // Debug multiple values
/// let (a, b) = dbg!(1, 2);  // returns (1, 2)
/// ```
#[macro_export]
macro_rules! dbg {
    ($val:expr $(,)?) => {{
        static DBG_ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        let id = *DBG_ID.get_or_init(|| {
            $crate::debug::DEBUG_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        });
        const DBG_LOC: &'static str = concat!(file!(), ":", line!());
        const DBG_EXPR: &'static str = stringify!($val);
        match $val {
            tmp => {
                $crate::debug::dbg::log_dbg(id, DBG_LOC, DBG_EXPR, &tmp);
                tmp
            }
        }
    }};
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

/// Value tracking macro that logs key-value pairs to the profiler.
///
/// Unlike `dbg!`, this macro takes a string key and returns a handle
/// with a `.set()` method. Values are grouped by key (not source location),
/// but each log entry records its source location for debugging.
///
/// # Examples
///
/// ```rust,ignore
/// use hotpath::val;
///
/// // Track a counter value
/// hotpath::val!("counter").set(&count);
///
/// // Track state changes
/// hotpath::val!("state").set(&current_state);
///
/// // Dynamic keys work too
/// let key = format!("counter_{}", id);
/// hotpath::val!(key).set(&value);
/// ```
#[macro_export]
macro_rules! val {
    ($key:expr) => {{
        const VAL_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::debug::val::ValHandle::new($key, VAL_LOC)
    }};
}

/// Gauge macro for tracking numeric values with set/inc/dec operations.
///
/// Returns a `GaugeHandle` that can be used to set, increment, or decrement
/// a numeric value. Gauges track the current value, min/max values, and
/// update history. Gauges are displayed in the Data Flow tab of the TUI.
///
/// # Examples
///
/// ```rust,ignore
/// use hotpath::gauge;
///
/// // Set an absolute value
/// hotpath::gauge!("queue_size").set(42.0);
///
/// // Increment/decrement with fluent API
/// hotpath::gauge!("active_connections").inc(1.0);
/// hotpath::gauge!("active_connections").dec(1.0);
///
/// // Chain operations
/// hotpath::gauge!("counter").set(0.0).inc(5.0).dec(2.0);
/// ```
#[macro_export]
macro_rules! gauge {
    ($key:expr) => {{
        const GAUGE_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::debug::gauge::GaugeHandle::new($key, GAUGE_LOC)
    }};
}

#[macro_export]
macro_rules! tokio_runtime {
    () => {
        hotpath::tokio_runtime::init_runtime_monitoring(&tokio::runtime::Handle::current());
    };
    ($handle:expr) => {
        hotpath::tokio_runtime::init_runtime_monitoring($handle);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_hotpath_is_send_sync() {
        is_send_sync::<HotpathGuard>();
    }
}
