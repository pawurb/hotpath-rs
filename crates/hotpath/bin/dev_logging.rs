#[cfg(feature = "dev")]
pub(crate) fn init_logging() {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let time_format =
        time::format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]").unwrap();
    let timer = tracing_subscriber::fmt::time::OffsetTime::new(offset, time_format);
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));

    std::fs::create_dir_all("log").expect("failed to create log directory");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("log/development.log")
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
pub(crate) fn init_logging() {}
