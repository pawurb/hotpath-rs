#[cfg(feature = "dev")]
#[allow(unused_imports)]
pub use tracing::{debug, error, info, trace, warn};

#[cfg(not(feature = "dev"))]
#[macro_export]
#[doc(hidden)]
macro_rules! __hotpath_noop_log {
    ($($tt:tt)*) => {{
        let _ = format_args!($($tt)*);
    }};
}

#[cfg(not(feature = "dev"))]
#[allow(unused_imports)]
pub use crate::__hotpath_noop_log as debug;
#[cfg(not(feature = "dev"))]
#[allow(unused_imports)]
pub use crate::__hotpath_noop_log as error;
#[cfg(not(feature = "dev"))]
#[allow(unused_imports)]
pub use crate::__hotpath_noop_log as info;
#[cfg(not(feature = "dev"))]
#[allow(unused_imports)]
pub use crate::__hotpath_noop_log as trace;
#[cfg(not(feature = "dev"))]
#[allow(unused_imports)]
pub use crate::__hotpath_noop_log as warn;

#[cfg(feature = "dev")]
pub static DEV_LOG_PATH: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
    std::env::var("HOTPATH_DEV_LOG_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("log/development.log"))
});

#[cfg(feature = "dev")]
#[allow(dead_code)]
pub fn init_logging() {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let time_format =
        time::format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]").unwrap();
    let timer = tracing_subscriber::fmt::time::OffsetTime::new(offset, time_format);
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));

    if let Some(parent) = DEV_LOG_PATH.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).expect("failed to create log directory");
        }
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&*DEV_LOG_PATH)
        .expect("failed to open log file");
    let file_layer = fmt::layer()
        .with_writer(log_file)
        .with_ansi(false)
        .with_timer(timer)
        .with_target(false)
        .with_thread_ids(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();
}

#[cfg(not(feature = "dev"))]
#[allow(dead_code)]
pub fn init_logging() {}
