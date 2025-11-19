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

// Re-export channels module when profiling is enabled
#[cfg(not(feature = "hotpath-off"))]
pub use lib_on::channels;

#[allow(dead_code)]
pub(crate) mod output;
pub use output::{
    format_bytes, format_duration, shorten_function_name, MetricType, MetricsDataJson, MetricsJson,
    MetricsProvider, ProfilingMode, Reporter, SamplesJson,
};

#[cfg(not(feature = "hotpath-off"))]
pub(crate) mod http_server;

// When hotpath is disabled with hotpath-off feature we import methods from lib_off, which are all no-op
#[cfg(feature = "hotpath-off")]
#[doc(inline)]
pub use lib_off::*;
#[cfg(feature = "hotpath-off")]
mod lib_off;
