//! Prometheus exporter (`hotpath-prometheus` feature), served from its own
//! tiny_http server (`HOTPATH_PROMETHEUS_HOST`/`HOTPATH_PROMETHEUS_PORT`,
//! auth via `HOTPATH_PROMETHEUS_AUTH_TOKEN`). One `GET /metrics` endpoint
//! with two bodies negotiated via the `Accept` header: protobuf with native
//! histograms (the high-resolution path) for scrapers that ask for it, text
//! exposition with coarse classic buckets for everything else. What
//! Prometheus ingests is decided by its own scrape config
//! (`scrape_native_histograms`), never here.

pub(crate) mod protobuf;

use std::fmt::Write;
use std::sync::{LazyLock, OnceLock};
use std::thread;

use tiny_http::{Header, Request, Response, Server};

use crate::lib_on::native_histograms::to_spans;

pub(crate) static PROMETHEUS_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_PROMETHEUS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6772)
});

/// Bind address for the exporter. Defaults to loopback; set to e.g. `0.0.0.0`
/// when a Prometheus container must reach the exporter through the Docker
/// bridge gateway (`host.docker.internal` on native Linux).
pub(crate) static PROMETHEUS_HOST: LazyLock<String> = LazyLock::new(|| {
    std::env::var("HOTPATH_PROMETHEUS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
});

static PROMETHEUS_AUTH_TOKEN: LazyLock<Option<String>> =
    LazyLock::new(|| crate::auth::token_from_env("HOTPATH_PROMETHEUS_AUTH_TOKEN"));

static PROMETHEUS_SERVER_STARTED: OnceLock<()> = OnceLock::new();
static PROMETHEUS_SERVER_ERROR: OnceLock<String> = OnceLock::new();

/// Native histogram resolution: bucket ratio `2^(1/8)` ~ 9%. Locked after
/// benchmarking schemas 2/3/4: conversion cost is schema-independent, and 3
/// keeps worst-case sparse bucket counts within common
/// `native_histogram_bucket_limit` settings where 4 does not.
pub(crate) const NATIVE_SCHEMA: i32 = 3;

/// Bucket boundaries (ns) for the classic-format fallback - log-spaced 1-3
/// steps, coarse on purpose: native histograms are the high-resolution path.
/// `+Inf` is emitted by the renderer, never stored here.
pub(crate) const FAST_LADDER_NS: &[u64] = &[
    250,            // 250ns
    1_000,          // 1µs
    3_000,          // 3µs
    10_000,         // 10µs
    30_000,         // 30µs
    100_000,        // 100µs
    300_000,        // 300µs
    1_000_000,      // 1ms
    3_000_000,      // 3ms
    10_000_000,     // 10ms
    30_000_000,     // 30ms
    100_000_000,    // 100ms
    300_000_000,    // 300ms
    1_000_000_000,  // 1s
    3_000_000_000,  // 3s
    10_000_000_000, // 10s
];

const TEXT_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";
const PROTOBUF_CONTENT_TYPE: &str =
    "application/vnd.google.protobuf; proto=io.prometheus.client.MetricFamily; encoding=delimited";

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn start_prometheus_server_once() {
    PROMETHEUS_SERVER_STARTED.get_or_init(|| {
        start_prometheus_server(*PROMETHEUS_PORT);
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn start_prometheus_server(port: u16) {
    LazyLock::force(&PROMETHEUS_AUTH_TOKEN);
    crate::dev_logging::init_logging();

    thread::Builder::new()
        .name("hp-prometheus".into())
        .spawn(move || {
            let _suspend = crate::lib_on::SuspendAllocTracking::new();
            let addr = format!("{}:{}", *PROMETHEUS_HOST, port);
            crate::dev_logging::info!("prometheus: exporter listening on {}", addr);
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

fn accepts_protobuf(accept: &str) -> bool {
    accept.contains("application/vnd.google.protobuf") && accept.contains("delimited")
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn handle_request(request: Request) {
    let provided = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str());
    if !check_auth_with_bearer(PROMETHEUS_AUTH_TOKEN.as_deref(), provided) {
        crate::dev_logging::warn!("prometheus: unauthorized request to {}", request.url());
        respond_text(request, 401, "Unauthorized\n");
        return;
    }

    let path = request.url().split('?').next().unwrap_or("");
    if path != "/metrics" {
        crate::dev_logging::warn!("prometheus: unknown path {}", path);
        respond_text(request, 404, "Not found\n");
        return;
    }

    let accept = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Accept"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    let user_agent = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("User-Agent"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();

    let protobuf = accepts_protobuf(&accept);
    let body = if protobuf {
        render_protobuf().map(|body| (body, PROTOBUF_CONTENT_TYPE))
    } else {
        render().map(|body| (body.into_bytes(), TEXT_CONTENT_TYPE))
    };

    match body {
        Some((body, content_type)) => {
            crate::dev_logging::info!(
                "prometheus: scrape format={} bytes={} user_agent={:?} accept={:?}",
                if protobuf { "protobuf" } else { "text" },
                body.len(),
                user_agent,
                accept
            );
            respond_bytes(request, body, content_type);
        }
        // An empty scrape would fabricate counter resets in Prometheus, so a
        // timed-out or not-yet-started worker yields an error status instead.
        None => {
            crate::dev_logging::warn!("prometheus: worker not ready, responding 503");
            respond_text(request, 503, "Profiler worker not ready\n");
        }
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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn render_protobuf() -> Option<Vec<u8>> {
    use crate::prometheus_server::protobuf::{
        label, Bucket, BucketSpan, Counter, Gauge, Histogram, Metric, MetricFamily, MetricType,
    };
    use prost::Message;

    let functions = crate::functions::get_functions_raw()?;

    let mut families = vec![
        MetricFamily {
            name: Some("hotpath_build_info".into()),
            help: Some("hotpath version info, value is always 1.".into()),
            r#type: Some(MetricType::Gauge as i32),
            metric: vec![Metric {
                label: vec![label("version", env!("CARGO_PKG_VERSION"))],
                gauge: Some(Gauge { value: Some(1.0) }),
                ..Default::default()
            }],
        },
        MetricFamily {
            name: Some("hotpath_uptime_seconds".into()),
            help: Some("Seconds since profiling started.".into()),
            r#type: Some(MetricType::Gauge as i32),
            metric: vec![Metric {
                gauge: Some(Gauge {
                    value: Some(seconds(crate::lib_on::current_elapsed_ns())),
                }),
                ..Default::default()
            }],
        },
    ];

    if !functions.is_empty() {
        families.push(MetricFamily {
            name: Some("hotpath_function_calls_total".into()),
            help: Some(
                "Total calls of each instrumented function, including calls skipped by time sampling."
                    .into(),
            ),
            r#type: Some(MetricType::Counter as i32),
            metric: functions
                .iter()
                .map(|f| Metric {
                    label: vec![label("function", f.name)],
                    counter: Some(Counter {
                        value: Some(f.count as f64),
                    }),
                    ..Default::default()
                })
                .collect(),
        });

        families.push(MetricFamily {
            name: Some("hotpath_function_duration_seconds".into()),
            help: Some("Duration of sampled calls of each instrumented function.".into()),
            r#type: Some(MetricType::Histogram as i32),
            metric: functions
                .iter()
                .map(|f| {
                    let (spans, deltas) = to_spans(&f.native_buckets);
                    Metric {
                        label: vec![label("function", f.name)],
                        histogram: Some(Histogram {
                            sample_count: Some(f.sampled_count),
                            sample_sum: Some(seconds(f.total_duration_ns)),
                            // +Inf is implicit in protobuf: the scraper derives
                            // it from sample_count.
                            bucket: FAST_LADDER_NS
                                .iter()
                                .zip(&f.bucket_counts)
                                .map(|(&boundary_ns, &cumulative)| Bucket {
                                    cumulative_count: Some(cumulative),
                                    upper_bound: Some(seconds(boundary_ns)),
                                })
                                .collect(),
                            schema: Some(NATIVE_SCHEMA),
                            zero_threshold: Some(0.0),
                            zero_count: Some(0),
                            positive_span: spans
                                .into_iter()
                                .map(|(offset, length)| BucketSpan {
                                    offset: Some(offset),
                                    length: Some(length),
                                })
                                .collect(),
                            positive_delta: deltas,
                        }),
                        ..Default::default()
                    }
                })
                .collect(),
        });
    }

    let mut out = Vec::with_capacity(16 * 1024);
    for family in &families {
        family.encode_length_delimited(&mut out).ok()?;
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
fn respond_bytes(request: Request, body: Vec<u8>, content_type: &str) {
    let mut response = Response::from_data(body);
    response.add_header(
        Header::from_bytes(b"Content-Type".as_slice(), content_type.as_bytes()).unwrap(),
    );
    let _ = request.respond(response);
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
    use crate::prometheus_server::{
        accepts_protobuf, check_auth_with_bearer, escape_label_value, seconds,
    };

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
        assert_eq!(seconds(3_000_000_000).to_string(), "3");
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

    #[test]
    fn protobuf_negotiation() {
        assert!(accepts_protobuf(
            "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited,text/plain;version=0.0.4;q=0.5"
        ));
        assert!(!accepts_protobuf("text/plain; version=0.0.4"));
        assert!(!accepts_protobuf(""));
    }
}
