//! hotpath-rs is a simple async Rust profiler. It instruments functions, channels, futures, and streams to quickly find bottlenecks and focus optimizations where they matter most.
//! It can provide actionable insights into time, memory, and data flow with minimal setup.
//! ## Setup & Usage
//! For a complete setup guide, examples, and advanced configuration, visit
//! [hotpath.rs](https://hotpath.rs).

#[cfg(all(
    feature = "hotpath-cpu",
    not(any(target_os = "macos", target_os = "linux"))
))]
compile_error!("the `hotpath-cpu` feature is only supported on macOS and Linux");

#[cfg(feature = "hotpath")]
#[doc(inline)]
pub use lib_on::*;
#[cfg(feature = "hotpath")]
mod lib_on;

#[cfg(feature = "hotpath")]
pub use lib_on::channels;
#[cfg(feature = "hotpath")]
pub use lib_on::futures;
#[cfg(feature = "hotpath")]
pub use lib_on::mutexes;
#[cfg(feature = "hotpath")]
pub use lib_on::streams;
#[cfg(all(feature = "hotpath", feature = "threads"))]
pub use lib_on::threads;
#[cfg(all(feature = "hotpath", feature = "tokio"))]
pub use lib_on::tokio_runtime;

#[cfg(any(feature = "hotpath", feature = "utils", feature = "tui"))]
pub(crate) mod output;
#[cfg(feature = "hotpath")]
pub use output::format_debug_truncated;
#[cfg(any(feature = "hotpath", feature = "utils", feature = "tui"))]
pub use output::{
    ceil_char_boundary, floor_char_boundary, format_bytes, format_count, format_duration,
    format_percentile_header, format_percentile_key, parse_bytes, parse_count, parse_duration,
    shorten_function_name, OutputDestination, ProfilingMode, MAX_LOG_LEN,
};

#[cfg(feature = "hotpath")]
pub(crate) mod output_on;

#[cfg(feature = "hotpath")]
pub(crate) mod metrics_server;

#[cfg(feature = "hotpath-mcp")]
pub(crate) mod mcp_server;

#[allow(dead_code)]
#[cfg(any(feature = "hotpath", feature = "utils", feature = "tui"))]
pub mod json;
#[cfg(any(feature = "hotpath", feature = "utils", feature = "tui"))]
pub use json::Route;

#[cfg(feature = "hotpath")]
pub(crate) mod instant;
#[cfg(feature = "hotpath")]
pub(crate) mod tid;

#[cfg(not(feature = "hotpath"))]
#[doc(inline)]
pub use lib_off::*;
#[cfg(not(feature = "hotpath"))]
mod lib_off;

#[cfg(not(feature = "hotpath"))]
pub use lib_off::channels;
#[cfg(not(feature = "hotpath"))]
pub use lib_off::futures;
#[cfg(not(feature = "hotpath"))]
pub use lib_off::streams;
#[cfg(not(feature = "hotpath"))]
pub use lib_off::threads;

/// Mirror of `std` paths so instrumented types can be used as drop-in
/// replacements by prefixing imports with `hotpath::wrap::` (e.g.
/// `hotpath::wrap::std::sync::RwLock`).
pub mod wrap {
    pub mod std {
        pub mod sync {
            #[cfg(not(feature = "hotpath"))]
            pub use crate::lib_off::mutexes::{Mutex, MutexGuard};
            #[cfg(not(feature = "hotpath"))]
            pub use crate::lib_off::rw_locks::{RwLock, RwLockReadGuard, RwLockWriteGuard};
            #[cfg(feature = "hotpath")]
            pub use crate::lib_on::mutexes::wrapper::std::{Mutex, MutexGuard};
            #[cfg(feature = "hotpath")]
            pub use crate::lib_on::rw_locks::wrapper::std::{
                RwLock, RwLockReadGuard, RwLockWriteGuard,
            };
        }
    }

    #[cfg(feature = "parking_lot")]
    pub mod parking_lot {
        #[cfg(not(feature = "hotpath"))]
        pub use crate::lib_off::parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
        #[cfg(feature = "hotpath")]
        pub use crate::lib_on::rw_locks::wrapper::parking_lot::{
            RwLock, RwLockReadGuard, RwLockWriteGuard,
        };
    }

    #[cfg(feature = "async-lock")]
    pub mod async_lock {
        #[cfg(not(feature = "hotpath"))]
        pub use crate::lib_off::async_lock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
        #[cfg(feature = "hotpath")]
        pub use crate::lib_on::rw_locks::wrapper::async_lock::{
            RwLock, RwLockReadGuard, RwLockWriteGuard,
        };
    }

    #[cfg(feature = "tokio")]
    pub mod tokio {
        pub mod sync {
            #[cfg(not(feature = "hotpath"))]
            pub use crate::lib_off::tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
            #[cfg(feature = "hotpath")]
            pub use crate::lib_on::rw_locks::wrapper::tokio::{
                RwLock, RwLockReadGuard, RwLockWriteGuard,
            };
        }
    }
}

mod shared;
pub use shared::{env_flag, Format, IntoF64, Section};

#[doc(hidden)]
pub mod dev_logging;
