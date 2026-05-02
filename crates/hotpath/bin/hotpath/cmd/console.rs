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

    #[arg(long, default_value_t = default_refresh_interval(), help = "Refresh interval in milliseconds (env: HOTPATH_TUI_REFRESH_INTERVAL_MS)")]
    pub refresh_interval: u64,
}

#[hotpath::measure_all]
impl ConsoleArgs {
    pub fn run(&self) -> Result<()> {
        hotpath::dev_logging::init_logging();

        #[cfg(feature = "hotpath")]
        demo::init();

        let mut app = App::new(&self.metrics_host, self.metrics_port, self.refresh_interval);

        let mut terminal = ratatui::init();

        let app_result = app.run(&mut terminal);

        ratatui::restore();

        app_result.map_err(|e| eyre::eyre!("TUI error: {}", e))
    }
}

impl Default for ConsoleArgs {
    fn default() -> Self {
        Self {
            metrics_port: default_metrics_port(),
            metrics_host: default_metrics_host(),
            refresh_interval: default_refresh_interval(),
        }
    }
}

fn default_metrics_port() -> u16 {
    std::env::var("HOTPATH_METRICS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6770)
}

fn default_refresh_interval() -> u64 {
    std::env::var("HOTPATH_TUI_REFRESH_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500)
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
