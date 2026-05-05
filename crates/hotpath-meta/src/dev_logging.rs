#[cfg(not(feature = "dev-meta"))]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_meta_noop_log {
    ($($tt:tt)*) => {{
        let _ = format_args!($($tt)*);
    }};
}

#[cfg(not(feature = "dev-meta"))]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_noop_log as debug;
#[cfg(not(feature = "dev-meta"))]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_noop_log as error;
#[cfg(not(feature = "dev-meta"))]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_noop_log as info;
#[cfg(not(feature = "dev-meta"))]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_noop_log as trace;
#[cfg(not(feature = "dev-meta"))]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_noop_log as warn;

#[cfg(feature = "dev-meta")]
pub static DEV_LOG_PATH: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
    std::env::var("HOTPATH_META_DEV_LOG_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("log/development.log"))
});

#[cfg(feature = "dev-meta")]
static WRITER: std::sync::OnceLock<Option<std::sync::Mutex<std::io::BufWriter<std::fs::File>>>> =
    std::sync::OnceLock::new();

#[cfg(feature = "dev-meta")]
#[allow(dead_code)]
pub fn init_logging() {
    let _ = WRITER.get_or_init(|| {
        if let Some(parent) = DEV_LOG_PATH.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).ok()?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&*DEV_LOG_PATH)
            .ok()?;
        Some(std::sync::Mutex::new(std::io::BufWriter::new(file)))
    });
}

#[cfg(feature = "dev-meta")]
#[doc(hidden)]
pub fn __write_log(level: &'static str, args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let writer = match WRITER.get() {
        Some(Some(w)) => w,
        _ => return,
    };
    let Ok(mut guard) = writer.lock() else {
        return;
    };
    let now = current_timestamp();
    let _ = writeln!(*guard, "{} {} {}", now, level, args);
    let _ = guard.flush();
}

#[cfg(feature = "dev-meta")]
fn current_timestamp() -> String {
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let now = time::OffsetDateTime::now_utc().to_offset(offset);
    let fmt = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
    now.format(&fmt).unwrap_or_default()
}

#[cfg(feature = "dev-meta")]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_meta_dev_log_trace {
    ($($tt:tt)*) => {{ $crate::dev_logging::__write_log("TRACE", std::format_args!($($tt)*)) }};
}
#[cfg(feature = "dev-meta")]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_meta_dev_log_debug {
    ($($tt:tt)*) => {{ $crate::dev_logging::__write_log("DEBUG", std::format_args!($($tt)*)) }};
}
#[cfg(feature = "dev-meta")]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_meta_dev_log_info {
    ($($tt:tt)*) => {{ $crate::dev_logging::__write_log("INFO", std::format_args!($($tt)*)) }};
}
#[cfg(feature = "dev-meta")]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_meta_dev_log_warn {
    ($($tt:tt)*) => {{ $crate::dev_logging::__write_log("WARN", std::format_args!($($tt)*)) }};
}
#[cfg(feature = "dev-meta")]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_meta_dev_log_error {
    ($($tt:tt)*) => {{ $crate::dev_logging::__write_log("ERROR", std::format_args!($($tt)*)) }};
}

#[cfg(feature = "dev-meta")]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_dev_log_debug as debug;
#[cfg(feature = "dev-meta")]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_dev_log_error as error;
#[cfg(feature = "dev-meta")]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_dev_log_info as info;
#[cfg(feature = "dev-meta")]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_dev_log_trace as trace;
#[cfg(feature = "dev-meta")]
#[allow(unused_imports)]
pub use crate::__hotpath_meta_dev_log_warn as warn;

#[cfg(not(feature = "dev-meta"))]
#[allow(dead_code)]
pub fn init_logging() {}
