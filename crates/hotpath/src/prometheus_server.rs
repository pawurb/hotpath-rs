//! Prometheus text-exposition endpoint, served from its own tiny_http server.
//! Opt-in via `HOTPATH_PROMETHEUS=true`, independent of the JSON metrics
//! server: separate bind address (`HOTPATH_PROMETHEUS_ADDR`), port
//! (`HOTPATH_PROMETHEUS_PORT`) and auth token
//! (`HOTPATH_PROMETHEUS_AUTH_TOKEN`).

use std::fmt::Write;
use std::sync::{LazyLock, OnceLock};
use std::thread;

use tiny_http::{Header, Request, Response, Server};

pub(crate) static PROMETHEUS_ENABLED: LazyLock<bool> =
    LazyLock::new(|| crate::shared::env_flag("HOTPATH_PROMETHEUS"));

pub(crate) static PROMETHEUS_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_PROMETHEUS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6772)
});

/// Bind address for the exporter. Defaults to loopback; set to e.g. `0.0.0.0`
/// when a Prometheus container must reach the exporter through the Docker
/// bridge gateway (`host.docker.internal` on native Linux).
pub(crate) static PROMETHEUS_ADDR: LazyLock<String> = LazyLock::new(|| {
    std::env::var("HOTPATH_PROMETHEUS_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string())
});

static PROMETHEUS_AUTH_TOKEN: LazyLock<Option<String>> =
    LazyLock::new(|| crate::auth::token_from_env("HOTPATH_PROMETHEUS_AUTH_TOKEN"));

static PROMETHEUS_SERVER_STARTED: OnceLock<()> = OnceLock::new();
static PROMETHEUS_SERVER_ERROR: OnceLock<String> = OnceLock::new();

/// Bucket boundaries (ns) for function duration histograms - log-spaced
/// 1-2.5-5 steps. `+Inf` is emitted by the renderer, never stored here.
pub(crate) const FAST_LADDER_NS: &[u64] = &[
    250,
    500,
    1_000,
    2_500,
    5_000,
    10_000,
    25_000,
    50_000,
    100_000,
    250_000,
    500_000,
    1_000_000,
    2_500_000,
    5_000_000,
    10_000_000,
    25_000_000,
    50_000_000,
    100_000_000,
    250_000_000,
    500_000_000,
    1_000_000_000,
    2_500_000_000,
    5_000_000_000,
    10_000_000_000,
];

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn start_prometheus_server_once() {
    if !*PROMETHEUS_ENABLED {
        return;
    }
    PROMETHEUS_SERVER_STARTED.get_or_init(|| {
        start_prometheus_server(*PROMETHEUS_PORT);
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn start_prometheus_server(port: u16) {
    LazyLock::force(&PROMETHEUS_AUTH_TOKEN);

    thread::Builder::new()
        .name("hp-prometheus".into())
        .spawn(move || {
            let _suspend = crate::lib_on::SuspendAllocTracking::new();
            let addr = format!("{}:{}", *PROMETHEUS_ADDR, port);
            let server = match Server::http(&addr) {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!(
                        "{} busy ({}), skipping Prometheus server start. Use HOTPATH_PROMETHEUS_PORT to change the port.",
                        addr, e
                    );
                    eprintln!("[hotpath - error] {}", msg);
                    let _ = PROMETHEUS_SERVER_ERROR.set(msg);
                    return;
                }
            };

            for request in server.incoming_requests() {
                handle_request(request);
            }
        })
        .expect("Failed to spawn Prometheus server thread");
}

/// Prometheus's `authorization` scrape config sends `Bearer <token>`, so
/// accept the bare token or a `Bearer `-prefixed one.
fn check_auth_with_bearer(expected: Option<&str>, provided: Option<&str>) -> bool {
    crate::auth::check_auth(expected, provided)
        || crate::auth::check_auth(expected, provided.and_then(|p| p.strip_prefix("Bearer ")))
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn handle_request(request: Request) {
    let provided = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str());
    if !check_auth_with_bearer(PROMETHEUS_AUTH_TOKEN.as_deref(), provided) {
        respond_text(request, 401, "Unauthorized\n");
        return;
    }

    let path = request.url().split('?').next().unwrap_or("");
    if path != "/metrics" {
        respond_text(request, 404, "Not found\n");
        return;
    }

    match render() {
        Some(body) => {
            let mut response = Response::from_string(body);
            response.add_header(
                Header::from_bytes(
                    b"Content-Type".as_slice(),
                    b"text/plain; version=0.0.4; charset=utf-8".as_slice(),
                )
                .unwrap(),
            );
            let _ = request.respond(response);
        }
        // An empty scrape would fabricate counter resets in Prometheus, so a
        // timed-out or not-yet-started worker yields an error status instead.
        None => respond_text(request, 503, "Profiler worker not ready\n"),
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn render() -> Option<String> {
    let functions = crate::functions::get_functions_raw()?;
    let mut out = String::with_capacity(16 * 1024);

    push_family(
        &mut out,
        "hotpath_build_info",
        "gauge",
        "hotpath version info, value is always 1.",
    );
    let _ = writeln!(
        out,
        "hotpath_build_info{{version=\"{}\"}} 1",
        escape_label_value(env!("CARGO_PKG_VERSION"))
    );

    push_family(
        &mut out,
        "hotpath_uptime_seconds",
        "gauge",
        "Seconds since profiling started.",
    );
    let _ = writeln!(
        out,
        "hotpath_uptime_seconds {}",
        seconds(crate::lib_on::current_elapsed_ns())
    );

    if !functions.is_empty() {
        push_family(
            &mut out,
            "hotpath_function_calls_total",
            "counter",
            "Total calls of each instrumented function, including calls skipped by time sampling.",
        );
        for f in &functions {
            let _ = writeln!(
                out,
                "hotpath_function_calls_total{{function=\"{}\"}} {}",
                escape_label_value(f.name),
                f.count
            );
        }

        push_family(
            &mut out,
            "hotpath_function_duration_seconds",
            "histogram",
            "Duration of sampled calls of each instrumented function.",
        );
        for f in &functions {
            let function = escape_label_value(f.name);
            for (&boundary_ns, &cumulative) in FAST_LADDER_NS.iter().zip(&f.bucket_counts) {
                let _ = writeln!(
                    out,
                    "hotpath_function_duration_seconds_bucket{{function=\"{}\",le=\"{}\"}} {}",
                    function,
                    seconds(boundary_ns),
                    cumulative
                );
            }
            let _ = writeln!(
                out,
                "hotpath_function_duration_seconds_bucket{{function=\"{}\",le=\"+Inf\"}} {}",
                function, f.sampled_count
            );
            let _ = writeln!(
                out,
                "hotpath_function_duration_seconds_sum{{function=\"{}\"}} {}",
                function,
                seconds(f.total_duration_ns)
            );
            let _ = writeln!(
                out,
                "hotpath_function_duration_seconds_count{{function=\"{}\"}} {}",
                function, f.sampled_count
            );
        }
    }

    Some(out)
}

fn push_family(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {} {}", name, help);
    let _ = writeln!(out, "# TYPE {} {}", name, kind);
}

/// f64 `Display` never uses scientific notation, which the exposition format
/// tolerates but some parsers of `le` label values do not.
fn seconds(ns: u64) -> f64 {
    ns as f64 / 1e9
}

fn escape_label_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn respond_text(request: Request, code: u16, msg: &str) {
    let mut response = Response::from_string(msg).with_status_code(code);
    response.add_header(
        Header::from_bytes(
            b"Content-Type".as_slice(),
            b"text/plain; charset=utf-8".as_slice(),
        )
        .unwrap(),
    );
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use crate::prometheus_server::{check_auth_with_bearer, escape_label_value, seconds};

    #[test]
    fn escapes_label_values() {
        assert_eq!(escape_label_value(r#"a\b"c"#), r#"a\\b\"c"#);
        assert_eq!(escape_label_value("a\nb"), r"a\nb");
        assert_eq!(escape_label_value("plain"), "plain");
    }

    #[test]
    fn seconds_formats_without_exponent() {
        assert_eq!(seconds(250).to_string(), "0.00000025");
        assert_eq!(seconds(1_000_000_000).to_string(), "1");
        assert_eq!(seconds(2_500_000_000).to_string(), "2.5");
    }

    #[test]
    fn bearer_prefix_accepted() {
        assert!(check_auth_with_bearer(Some("secret"), Some("secret")));
        assert!(check_auth_with_bearer(
            Some("secret"),
            Some("Bearer secret")
        ));
        assert!(!check_auth_with_bearer(
            Some("secret"),
            Some("Bearer wrong")
        ));
        assert!(!check_auth_with_bearer(Some("secret"), None));
        assert!(check_auth_with_bearer(None, None));
    }
}
