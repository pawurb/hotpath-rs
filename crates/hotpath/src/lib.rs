//! A lightweight, easy-to-configure Rust profiler that shows exactly where your code spends time and allocates memory.
//! Instrument any function or code block with to quickly spot bottlenecks, and focus your optimizations where they matter most.
//! ## Setup & Usage
//! For a complete setup guide, examples, and advanced configuration, see the
//! [GitHub repository](https://github.com/pawurb/hotpath).

#[cfg(not(feature = "hotpath-off"))]
#[doc(inline)]
pub use lib_on::*;
#[cfg(not(feature = "hotpath-off"))]
mod lib_on;

#[cfg(not(feature = "hotpath-off"))]
pub use lib_on::channels;
#[cfg(not(feature = "hotpath-off"))]
pub use lib_on::streams;
#[cfg(all(not(feature = "hotpath-off"), feature = "threads"))]
pub use lib_on::threads;

#[allow(dead_code)]
pub(crate) mod output;
pub use output::{
    format_bytes, format_duration, shorten_function_name, FunctionLogsJson, FunctionsDataJson,
    FunctionsJson, MetricType, MetricsProvider, ProfilingMode, Reporter,
};

#[cfg(not(feature = "hotpath-off"))]
pub(crate) mod http_server;
#[cfg(not(feature = "hotpath-off"))]
pub use http_server::Route;

#[cfg(not(feature = "hotpath-off"))]
pub(crate) mod tid;

// When hotpath is disabled with hotpath-off feature we import methods from lib_off, which are all no-op
#[cfg(feature = "hotpath-off")]
#[doc(inline)]
pub use lib_off::*;
#[cfg(feature = "hotpath-off")]
mod lib_off;
