//! hotpath-rs is a simple async Rust profiler. It instruments functions, channels, futures, and streams to quickly find bottlenecks and focus optimizations where they matter most.
//! It can provide actionable insights into time, memory, and data flow with minimal setup.
//! ## Setup & Usage
//! For a complete setup guide, examples, and advanced configuration, visit
//! [hotpath.rs](https://hotpath.rs).

// Meta crate mirrors the main crate; some code is conditionally dead
// depending on feature combinations (e.g. alloc code without global_allocator).
#![allow(dead_code)]

#[cfg(all(
    feature = "hotpath-cpu-meta",
    not(any(target_os = "macos", target_os = "linux"))
))]
compile_error!("the `hotpath-cpu-meta` feature is only supported on macOS and Linux");

#[cfg(feature = "hotpath-meta")]
#[doc(inline)]
pub use lib_on::*;
#[cfg(feature = "hotpath-meta")]
mod lib_on;

#[cfg(feature = "hotpath-meta")]
pub use lib_on::channels;
#[cfg(feature = "hotpath-meta")]
pub use lib_on::futures;
#[cfg(feature = "hotpath-meta")]
pub use lib_on::mutexes;
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
#[doc(hidden)]
pub mod instant;
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

/// Mirror of `std` paths so instrumented types can be used as drop-in
/// replacements by prefixing imports with `hotpath_meta::wrap::` (e.g.
/// `hotpath_meta::wrap::std::sync::RwLock`).
pub mod wrap {
    pub mod std {
        pub mod sync {
            #[cfg(not(feature = "hotpath-meta"))]
            pub use crate::lib_off::mutexes::{Mutex, MutexGuard};
            #[cfg(not(feature = "hotpath-meta"))]
            pub use crate::lib_off::rw_locks::{RwLock, RwLockReadGuard, RwLockWriteGuard};
            #[cfg(feature = "hotpath-meta")]
            pub use crate::lib_on::mutexes::wrapper::std::{Mutex, MutexGuard};
            #[cfg(feature = "hotpath-meta")]
            pub use crate::lib_on::rw_locks::wrapper::std::{
                RwLock, RwLockReadGuard, RwLockWriteGuard,
            };
        }
    }

    /// Instrumented crossbeam channel endpoints for `channel!(..., wrap = true)`.
    /// With `hotpath-meta` enabled these are the instrumented wrappers; otherwise
    /// `channel!` is a no-op and the endpoints are the raw crossbeam types, so the
    /// alias resolves the same way regardless of feature configuration.
    #[cfg(feature = "crossbeam")]
    pub mod crossbeam {
        #[cfg(feature = "hotpath-meta")]
        pub use crate::lib_on::channels::wrapper::crossbeam_wrap::{Receiver, Sender};
        #[cfg(not(feature = "hotpath-meta"))]
        pub use crossbeam_channel::{Receiver, Sender};
    }
}

mod shared;
pub use shared::{env_flag, Format, IntoF64, Section};

#[doc(hidden)]
pub mod dev_logging;
