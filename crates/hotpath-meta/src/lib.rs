//! hotpath-rs is a simple async Rust profiler. It instruments functions, channels, futures, and streams to quickly find bottlenecks and focus optimizations where they matter most.
//! It can provide actionable insights into time, memory, and data flow with minimal setup.
//! ## Setup & Usage
//! For a complete setup guide, examples, and advanced configuration, visit
//! [hotpath.rs](https://hotpath.rs).

// Meta crate mirrors the main crate; some code is conditionally dead
// depending on feature combinations (e.g. alloc code without global_allocator).
#![allow(dead_code)]

#[cfg(feature = "hotpath-meta")]
#[doc(inline)]
pub use lib_on::*;
#[cfg(feature = "hotpath-meta")]
mod lib_on;

#[cfg(feature = "hotpath-meta")]
pub use lib_on::channels;
#[cfg(feature = "hotpath-meta")]
pub use lib_on::data_flow;
#[cfg(feature = "hotpath-meta")]
pub use lib_on::futures;
#[cfg(feature = "hotpath-meta")]
pub use lib_on::streams;
#[cfg(all(feature = "hotpath-meta", feature = "threads"))]
pub use lib_on::threads;
#[cfg(all(feature = "hotpath-meta", feature = "tokio"))]
pub use lib_on::tokio_runtime;

#[cfg(any(feature = "hotpath-meta", feature = "tui"))]
pub(crate) mod output;
#[cfg(feature = "hotpath-meta")]
pub use output::format_debug_truncated;
#[cfg(any(feature = "hotpath-meta", feature = "tui"))]
pub use output::{
    ceil_char_boundary, floor_char_boundary, format_bytes, format_count, format_duration,
    format_percentile_header, format_percentile_key, parse_bytes, parse_count, parse_duration,
    shorten_function_name, OutputDestination, ProfilingMode, MAX_LOG_LEN,
};

#[cfg(feature = "hotpath-meta")]
pub(crate) mod output_on;

#[cfg(feature = "hotpath-meta")]
pub(crate) mod metrics_server;

#[cfg(feature = "hotpath-mcp-meta")]
pub(crate) mod mcp_server;

#[allow(dead_code)]
#[cfg(any(feature = "hotpath-meta", feature = "tui"))]
pub mod json;
#[cfg(any(feature = "hotpath-meta", feature = "tui"))]
pub use json::Route;

#[cfg(feature = "hotpath-meta")]
pub(crate) mod instant;
#[cfg(feature = "hotpath-meta")]
pub(crate) mod tid;

#[cfg(not(feature = "hotpath-meta"))]
#[doc(inline)]
pub use lib_off::*;
#[cfg(not(feature = "hotpath-meta"))]
mod lib_off;

#[cfg(not(feature = "hotpath-meta"))]
pub use lib_off::channels;
#[cfg(not(feature = "hotpath-meta"))]
pub use lib_off::futures;
#[cfg(not(feature = "hotpath-meta"))]
pub use lib_off::streams;
#[cfg(not(feature = "hotpath-meta"))]
pub use lib_off::threads;

mod shared;
pub use shared::{Format, IntoF64, Section};
