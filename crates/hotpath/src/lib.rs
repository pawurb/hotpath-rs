//! hotpath-rs is a simple async Rust profiler. It instruments functions, channels, futures, and streams to quickly find bottlenecks and focus optimizations where they matter most.
//! It can provide actionable insights into time, memory, and data flow with minimal setup.
//! ## Setup & Usage
//! For a complete setup guide, examples, and advanced configuration, visit
//! [hotpath.rs](https://hotpath.rs).

/// Output format for profiling reports.
///
/// This enum specifies how profiling results should be displayed when the program exits.
///
/// # Variants
///
/// * `Table` - Human-readable table format (default)
/// * `Json` - JSON format
/// * `JsonPretty` - Pretty-printed JSON format
#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    #[default]
    Table,
    Json,
    JsonPretty,
}

#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
#[doc(inline)]
pub use lib_on::*;
#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
mod lib_on;

#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
pub use lib_on::channels;
#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
pub use lib_on::futures;
#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
pub use lib_on::streams;
#[cfg(all(feature = "hotpath", not(feature = "hotpath-off"), feature = "threads"))]
pub use lib_on::threads;

#[cfg(any(feature = "hotpath", feature = "ci", feature = "tui"))]
pub(crate) mod output;
#[cfg(any(feature = "hotpath", feature = "ci", feature = "tui"))]
pub use output::{
    ceil_char_boundary, floor_char_boundary, format_bytes, format_duration, shorten_function_name,
    truncate_result, FunctionLogsJson, FunctionsDataJson, FunctionsJson, MetricType,
    MetricsProvider, ProfilingMode, Reporter, MAX_RESULT_LEN,
};

#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
pub(crate) mod output_on;

#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
pub(crate) mod metrics_server;

#[cfg(all(feature = "hotpath-mcp", not(feature = "hotpath-off")))]
pub(crate) mod mcp_server;

#[cfg(any(feature = "hotpath", feature = "ci", feature = "tui"))]
pub mod json;
#[cfg(any(feature = "hotpath", feature = "ci", feature = "tui"))]
pub use json::Route;

#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
pub(crate) mod tid;

// When hotpath feature is not enabled or hotpath-off is enabled, use no-op stubs
#[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
#[doc(inline)]
pub use lib_off::*;
#[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
mod lib_off;

#[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
pub use lib_off::channels;
#[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
pub use lib_off::futures;
#[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
pub use lib_off::streams;
