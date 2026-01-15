mod app;
mod constants;
#[cfg(feature = "hotpath")]
pub mod demo;
mod events;
mod http_worker;
mod input;
mod views;
mod widgets;

use app::App;
use clap::Parser;
use eyre::Result;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Parser)]
pub struct ConsoleArgs {
    #[arg(
        long,
        default_value_t = default_metrics_port(),
        help = "Port where the metrics HTTP server is running (env: HOTPATH_METRICS_PORT)"
    )]
    pub metrics_port: u16,

    #[arg(
        long,
        default_value_t = default_metrics_host(),
        value_parser = validate_metrics_host,
        help = "Host URL where the metrics HTTP server is running (env: HOTPATH_METRICS_HOST)"
    )]
    pub metrics_host: String,

    #[arg(long, default_value_t = 500, help = "Refresh interval in milliseconds")]
    pub refresh_interval: u64,
}

#[hotpath::measure_all]
impl ConsoleArgs {
    pub fn run(&self) -> Result<()> {
        init_logging();

        #[cfg(feature = "hotpath")]
        demo::init();

        let mut app = App::new(&self.metrics_host, self.metrics_port, self.refresh_interval);

        // Use modern ratatui initialization
        let mut terminal = ratatui::init();

        let app_result = app.run(&mut terminal);

        // Use modern ratatui restoration
        ratatui::restore();

        app_result.map_err(|e| eyre::eyre!("TUI error: {}", e))
    }
}

fn init_logging() {
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let time_format =
        time::format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]").unwrap();
    let timer = tracing_subscriber::fmt::time::OffsetTime::new(offset, time_format);
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));

    std::fs::create_dir_all("log").expect("failed to create log directory");
    let log_file = std::fs::File::create("log/development.log").expect("failed to create log file");
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

fn default_metrics_port() -> u16 {
    std::env::var("HOTPATH_METRICS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6770)
}

fn default_metrics_host() -> String {
    std::env::var("HOTPATH_METRICS_HOST").unwrap_or_else(|_| "http://localhost".to_string())
}

fn validate_metrics_host(s: &str) -> Result<String, String> {
    let s = s.trim();

    if s.is_empty() {
        return Err("metrics host cannot be empty".to_string());
    }

    let after_scheme = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .ok_or_else(|| {
            format!(
                "--metrics-host must start with 'http://' or 'https://', got: {}",
                s
            )
        })?;

    if after_scheme.is_empty() {
        return Err("metrics host must include a hostname after the scheme".to_string());
    }

    let host_part = after_scheme.split('/').next().unwrap_or("");

    if host_part.contains(':') {
        return Err(format!(
            "metrics host should not include a port (use --metrics-port instead), got: {}",
            s
        ));
    }

    if host_part.is_empty() {
        return Err("metrics host must include a valid hostname".to_string());
    }

    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hosts() {
        let cases = [
            ("http://localhost", "http://localhost"),
            ("https://localhost", "https://localhost"),
            ("http://192.168.1.1", "http://192.168.1.1"),
            ("https://example.com", "https://example.com"),
            ("http://localhost/", "http://localhost/"),
            ("  http://localhost  ", "http://localhost"),
        ];

        for (input, expected) in cases {
            assert_eq!(
                validate_metrics_host(input),
                Ok(expected.to_string()),
                "failed for input: {input}"
            );
        }
    }

    #[test]
    fn test_invalid_hosts() {
        let cases = [
            "",
            "   ",
            "localhost",
            "ftp://localhost",
            "http://",
            "https://",
            "http://localhost:8080",
            "https://example.com:443",
        ];

        for input in cases {
            assert!(
                validate_metrics_host(input).is_err(),
                "expected error for input: {input}"
            );
        }
    }
}
