//! Prometheus exporter (`hotpath-prometheus` feature), served from its own
//! tiny_http server (`HOTPATH_PROMETHEUS_HOST`/`HOTPATH_PROMETHEUS_PORT`,
//! auth via `HOTPATH_PROMETHEUS_AUTH_TOKEN`). One `GET /metrics` endpoint
//! with two bodies negotiated via the `Accept` header: protobuf with native
//! histograms (the high-resolution path) for scrapers that ask for it, text
//! exposition with coarse classic buckets for everything else. What
//! Prometheus ingests is decided by its own scrape config
//! (`scrape_native_histograms`), never here.

pub(crate) mod families;
pub(crate) mod protobuf;

use std::sync::{LazyLock, OnceLock};
use std::thread;

use tiny_http::{Header, Request, Response, Server};

use crate::prometheus_server::families::{
    to_protobuf, to_text, Family, FamilyKind, HistogramValue, Sample, SampleValue,
};

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

/// Bucket boundaries (ns) for I/O-bound latencies (SQL queries, HTTP client
/// requests, server responses) - same log-spaced 1-3 stepping as
/// `FAST_LADDER_NS`, shifted up: sub-100µs I/O is indistinguishably "fast" and
/// a p99 above the top boundary would clip to it and silently understate.
pub(crate) const SLOW_LADDER_NS: &[u64] = &[
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
    30_000_000_000, // 30s
    60_000_000_000, // 60s
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
    let body = collect_families().and_then(|families| {
        if protobuf {
            to_protobuf(&families).map(|body| (body, PROTOBUF_CONTENT_TYPE))
        } else {
            Some((to_text(&families).into_bytes(), TEXT_CONTENT_TYPE))
        }
    });

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

/// Builds every metric family for the current scrape. `None` when the
/// functions snapshot cannot be fetched (worker not started or timed out) -
/// the caller responds 503 instead of serving an empty scrape.
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_families() -> Option<Vec<Family>> {
    let functions = crate::functions::get_functions_raw()?;

    let mut families = vec![
        Family {
            name: "hotpath_build_info",
            help: "hotpath version info, value is always 1.",
            kind: FamilyKind::Gauge,
            samples: vec![Sample {
                labels: vec![("version", env!("CARGO_PKG_VERSION").to_string())],
                value: SampleValue::Scalar(1.0),
            }],
        },
        Family {
            name: "hotpath_uptime_seconds",
            help: "Seconds since profiling started.",
            kind: FamilyKind::Gauge,
            samples: vec![Sample {
                labels: vec![],
                value: SampleValue::Scalar(seconds(crate::lib_on::current_elapsed_ns())),
            }],
        },
    ];

    if !functions.is_empty() {
        families.push(Family {
            name: "hotpath_function_calls_total",
            help: "Total calls of each instrumented function, including calls skipped by time sampling.",
            kind: FamilyKind::Counter,
            samples: functions
                .iter()
                .map(|f| Sample {
                    labels: vec![("function", f.name.to_string())],
                    value: SampleValue::Scalar(f.count as f64),
                })
                .collect(),
        });

        families.push(Family {
            name: "hotpath_function_duration_seconds",
            help: "Duration of sampled calls of each instrumented function.",
            kind: FamilyKind::Histogram,
            samples: functions
                .into_iter()
                .map(|f| Sample {
                    labels: vec![("function", f.name.to_string())],
                    value: SampleValue::Histogram(HistogramValue {
                        sample_count: f.sampled_count,
                        sum: seconds(f.total_duration_ns),
                        classic_buckets: classic_pairs(FAST_LADDER_NS, f.bucket_counts),
                        native_buckets: f.native_buckets,
                    }),
                })
                .collect(),
        });
    }

    collect_sql(&mut families);
    collect_http(&mut families);
    collect_server(&mut families);
    collect_mutexes(&mut families);
    collect_rw_locks(&mut families);
    collect_channels(&mut families);
    collect_streams(&mut families);

    Some(families)
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_mutexes(families: &mut Vec<Family>) {
    // An empty key marks a placeholder whose `Created` event has not been
    // processed yet: no identity labels, so exporting it would fabricate
    // series (and duplicates, given several placeholders). Its counts appear
    // once registration lands, at worst one sweep later.
    let mut entries = crate::lib_on::mutexes::get_sorted_mutex_entries();
    entries.retain(|e| !e.key.is_empty());
    if entries.is_empty() {
        return;
    }

    let labels = |e: &crate::lib_on::mutexes::MutexEntry| {
        call_site_labels(e.key, e.source, e.label.as_deref(), e.iter)
    };

    families.push(Family {
        name: "hotpath_mutex_acquisitions_total",
        help: "Total lock acquisitions per mutex call site, including acquisitions skipped by time sampling.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_mutex_wait_seconds",
        help: "Time sampled acquisitions spent waiting to acquire the mutex.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Histogram(HistogramValue {
                    sample_count: e.sampled_count,
                    sum: seconds(e.wait_total_nanos),
                    classic_buckets: classic_pairs(
                        FAST_LADDER_NS,
                        e.classic_wait_buckets(FAST_LADDER_NS),
                    ),
                    native_buckets: e.native_wait_buckets(NATIVE_SCHEMA),
                }),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_mutex_acquire_seconds",
        help: "Time sampled acquisitions held the mutex.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Histogram(HistogramValue {
                    sample_count: e.sampled_count,
                    sum: seconds(e.acquire_total_nanos),
                    classic_buckets: classic_pairs(
                        FAST_LADDER_NS,
                        e.classic_acquire_buckets(FAST_LADDER_NS),
                    ),
                    native_buckets: e.native_acquire_buckets(NATIVE_SCHEMA),
                }),
            })
            .collect(),
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_rw_locks(families: &mut Vec<Family>) {
    use crate::lib_on::rw_locks::RwLockKind;

    let mut entries = crate::lib_on::rw_locks::get_sorted_rw_lock_entries();
    entries.retain(|e| !e.key.is_empty());
    if entries.is_empty() {
        return;
    }

    const KINDS: [(RwLockKind, &str); 2] =
        [(RwLockKind::Read, "read"), (RwLockKind::Write, "write")];

    let labels = |e: &crate::lib_on::rw_locks::RwLockEntry, op: &str| {
        let mut labels = call_site_labels(e.key, e.source, e.label.as_deref(), e.iter);
        labels.push(("op", op.to_string()));
        labels
    };

    families.push(Family {
        name: "hotpath_rwlock_acquisitions_total",
        help: "Total lock acquisitions per rwlock call site and side, including acquisitions skipped by time sampling.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .flat_map(|e| {
                KINDS.map(|(kind, op)| Sample {
                    labels: labels(e, op),
                    value: SampleValue::Scalar(e.count(kind) as f64),
                })
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_rwlock_wait_seconds",
        help: "Time sampled acquisitions spent waiting to acquire the rwlock, per side.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .flat_map(|e| {
                KINDS.map(|(kind, op)| Sample {
                    labels: labels(e, op),
                    value: SampleValue::Histogram(HistogramValue {
                        sample_count: e.sampled_count(kind),
                        sum: seconds(e.wait_total_nanos(kind)),
                        classic_buckets: classic_pairs(
                            FAST_LADDER_NS,
                            e.classic_wait_buckets(kind, FAST_LADDER_NS),
                        ),
                        native_buckets: e.native_wait_buckets(kind, NATIVE_SCHEMA),
                    }),
                })
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_rwlock_acquire_seconds",
        help: "Time sampled acquisitions held the rwlock, per side.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .flat_map(|e| {
                KINDS.map(|(kind, op)| Sample {
                    labels: labels(e, op),
                    value: SampleValue::Histogram(HistogramValue {
                        sample_count: e.sampled_count(kind),
                        sum: seconds(e.acquire_total_nanos(kind)),
                        classic_buckets: classic_pairs(
                            FAST_LADDER_NS,
                            e.classic_acquire_buckets(kind, FAST_LADDER_NS),
                        ),
                        native_buckets: e.native_acquire_buckets(kind, NATIVE_SCHEMA),
                    }),
                })
            })
            .collect(),
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_channels(families: &mut Vec<Family>) {
    let mut entries = crate::lib_on::channels::get_sorted_channel_entries();
    entries.retain(|e| !e.key.is_empty());
    if entries.is_empty() {
        return;
    }

    // Default-mode entries are keyed by call site plus payload type, so the
    // payload is part of the identity: a generic helper creating channels for
    // two payload types from one site yields two entries.
    let labels = |e: &crate::lib_on::channels::ChannelEntry| {
        let mut labels = call_site_labels(e.key, e.source, e.label.as_deref(), e.iter);
        labels.push(("type", e.channel_type.to_string()));
        labels.push(("payload", e.type_name.to_string()));
        labels
    };

    families.push(Family {
        name: "hotpath_channel_sent_total",
        help: "Messages sent per channel call site.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.sent_count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_channel_received_total",
        help: "Messages received per channel call site.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.received_count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_channel_instances",
        help: "Channel instances created at this call site since start.",
        kind: FamilyKind::Gauge,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.instances as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_channel_closed_instances",
        help: "Channel instances created at this call site that have closed.",
        kind: FamilyKind::Gauge,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.closed_instances as f64),
            })
            .collect(),
    });

    let queue_sizes: Vec<Sample> = entries
        .iter()
        .filter_map(|e| {
            e.queue_size.map(|size| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(size as f64),
            })
        })
        .collect();
    if !queue_sizes.is_empty() {
        families.push(Family {
            name: "hotpath_channel_queue_size",
            help: "Messages currently sent but not yet received.",
            kind: FamilyKind::Gauge,
            samples: queue_sizes,
        });
    }

    let max_queue_sizes: Vec<Sample> = entries
        .iter()
        .filter_map(|e| {
            e.max_queue_size.map(|size| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(size as f64),
            })
        })
        .collect();
    if !max_queue_sizes.is_empty() {
        families.push(Family {
            name: "hotpath_channel_max_queue_size",
            help: "Since-start high-water mark of the queue size, not a windowed maximum.",
            kind: FamilyKind::Gauge,
            samples: max_queue_sizes,
        });
    }

    let proc: Vec<Sample> = entries
        .iter()
        .filter(|e| e.has_proc_hist())
        .map(|e| Sample {
            labels: labels(e),
            value: SampleValue::Histogram(HistogramValue {
                sample_count: e.proc_sampled_count,
                sum: seconds(e.proc_total_nanos),
                classic_buckets: classic_pairs(
                    FAST_LADDER_NS,
                    e.classic_proc_buckets(FAST_LADDER_NS),
                ),
                native_buckets: e.native_proc_buckets(NATIVE_SCHEMA),
            }),
        })
        .collect();
    if !proc.is_empty() {
        families.push(Family {
            name: "hotpath_channel_proc_seconds",
            help: "Delay between send and sampled receive of a message (wrap mode only).",
            kind: FamilyKind::Histogram,
            samples: proc,
        });
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_streams(families: &mut Vec<Family>) {
    let mut entries = crate::lib_on::streams::get_sorted_stream_stats();
    entries.retain(|e| !e.key.is_empty());
    if entries.is_empty() {
        return;
    }

    // Stream entries are keyed by call site plus item type - same identity
    // rule as channels.
    let labels = |e: &crate::lib_on::streams::StreamStats| {
        let mut labels = call_site_labels(e.key, e.source, e.label.as_deref(), e.iter);
        labels.push(("payload", e.type_name.to_string()));
        labels
    };

    families.push(Family {
        name: "hotpath_stream_items_total",
        help: "Items yielded per stream call site.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.items_yielded as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_stream_instances",
        help: "Stream instances created at this call site since start.",
        kind: FamilyKind::Gauge,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.instances as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_stream_closed_instances",
        help: "Stream instances created at this call site that have closed.",
        kind: FamilyKind::Gauge,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.closed_instances as f64),
            })
            .collect(),
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_sql(families: &mut Vec<Family>) {
    let entries = crate::lib_on::sql::get_sorted_sql_entries();
    if entries.is_empty() {
        return;
    }

    // Entries are keyed (route, source, normalized query), so these labels
    // expose the store's native granularity without adding series. The
    // normalized text itself is the query's identity - unlike the per-process
    // entry id it aggregates correctly across instances and restarts.
    let query_cap = *crate::output::MAX_LOG_LEN;
    let labels = |e: &crate::lib_on::sql::SqlEntry| {
        vec![
            ("query", query_label(&e.query, query_cap)),
            ("source", e.source.unwrap_or_default().to_string()),
            ("route", e.route.unwrap_or_default().to_string()),
        ]
    };

    families.push(Family {
        name: "hotpath_sql_queries_total",
        help: "Total executions of each tracked SQL query.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_sql_duration_seconds",
        help: "Duration of each tracked SQL query, labeled by its normalized text.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Histogram(HistogramValue {
                    sample_count: e.count,
                    sum: seconds(e.total_nanos),
                    classic_buckets: classic_pairs(
                        SLOW_LADDER_NS,
                        e.classic_buckets(SLOW_LADDER_NS),
                    ),
                    native_buckets: e.native_buckets(NATIVE_SCHEMA),
                }),
            })
            .collect(),
    });
}

/// Normalized query text as a label value. Text over `cap` chars is truncated
/// with an FNV-1a hash of the full text appended, so two long queries sharing
/// a prefix cannot collapse into one series (duplicate label sets are invalid
/// exposition).
fn query_label(query: &str, cap: usize) -> String {
    if query.chars().count() <= cap {
        return query.to_string();
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in query.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let truncated: String = query.chars().take(cap).collect();
    format!("{}...{:016x}", truncated, hash)
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_http(families: &mut Vec<Family>) {
    let entries = crate::lib_on::http::get_sorted_http_entries();
    if entries.is_empty() {
        return;
    }

    // Entries are keyed (route, source, endpoint), so all three labels are
    // needed to keep series distinct; aggregate with sum by (endpoint) in
    // PromQL for the per-endpoint view.
    let labels = |e: &crate::lib_on::http::HttpEntry| {
        vec![
            ("endpoint", e.endpoint.clone()),
            ("source", e.source.unwrap_or_default().to_string()),
            ("route", e.route.unwrap_or_default().to_string()),
        ]
    };

    families.push(Family {
        name: "hotpath_http_requests_total",
        help: "Total outbound HTTP requests per normalized endpoint.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_http_errors_total",
        help: "Outbound HTTP transport errors plus responses with status >= 400.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Scalar(e.error_count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_http_duration_seconds",
        help: "Duration of outbound HTTP requests per normalized endpoint.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: labels(e),
                value: SampleValue::Histogram(HistogramValue {
                    sample_count: e.count,
                    sum: seconds(e.total_nanos),
                    classic_buckets: classic_pairs(
                        SLOW_LADDER_NS,
                        e.classic_buckets(SLOW_LADDER_NS),
                    ),
                    native_buckets: e.native_buckets(NATIVE_SCHEMA),
                }),
            })
            .collect(),
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn collect_server(families: &mut Vec<Family>) {
    let entries = crate::lib_on::server::get_sorted_server_entries();
    if entries.is_empty() {
        return;
    }

    let route_labels = |e: &crate::lib_on::server::ServerEntry| vec![("route", e.route.clone())];

    families.push(Family {
        name: "hotpath_server_requests_total",
        help: "Total requests served per matched route template.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: route_labels(e),
                value: SampleValue::Scalar(e.count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_server_responses_total",
        help: "Responses per route with a 4xx or 5xx status; other statuses are not classified.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .flat_map(|e| {
                [("4xx", e.status_4xx), ("5xx", e.status_5xx)].map(|(class, count)| Sample {
                    labels: vec![("route", e.route.clone()), ("class", class.to_string())],
                    value: SampleValue::Scalar(count as f64),
                })
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_server_duration_seconds",
        help: "Duration of served requests per route, until the response head is produced.",
        kind: FamilyKind::Histogram,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: route_labels(e),
                value: SampleValue::Histogram(HistogramValue {
                    sample_count: e.count,
                    sum: seconds(e.total_nanos),
                    classic_buckets: classic_pairs(
                        SLOW_LADDER_NS,
                        e.classic_buckets(SLOW_LADDER_NS),
                    ),
                    native_buckets: e.native_buckets(NATIVE_SCHEMA),
                }),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_server_scoped_requests_total",
        help: "Completed requests that carried a route scope; the denominator for per-request SQL/HTTP rates.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: route_labels(e),
                value: SampleValue::Scalar(e.scoped_count as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_server_sql_calls_total",
        help: "SQL queries issued by route-scoped requests; divide by hotpath_server_scoped_requests_total for queries per request.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: route_labels(e),
                value: SampleValue::Scalar(e.sql_calls as f64),
            })
            .collect(),
    });

    families.push(Family {
        name: "hotpath_server_http_calls_total",
        help: "Outbound HTTP requests issued by route-scoped requests; divide by hotpath_server_scoped_requests_total for requests per request.",
        kind: FamilyKind::Counter,
        samples: entries
            .iter()
            .map(|e| Sample {
                labels: route_labels(e),
                value: SampleValue::Scalar(e.http_calls as f64),
            })
            .collect(),
    });
}

/// Shared call-site identity labels for entries keyed by wrapper macro call
/// site (locks, channels, streams) - a field-for-field mirror of the store's
/// entry identity, so distinct entries can never collapse into duplicate
/// series. `source` is the user-facing `file:line`; `col` is the call-site
/// column (two invocations on one physical line share `source`); `iter` is
/// the instantiation index for call sites that produce one entry per
/// instantiation; `label` is the user's label verbatim. Dashboards aggregate
/// the discriminators away with `sum by (source, label)`.
fn call_site_labels(
    key: &str,
    source: &str,
    label: Option<&str>,
    iter: u32,
) -> Vec<(&'static str, String)> {
    vec![
        ("source", source.to_string()),
        ("label", label.unwrap_or_default().to_string()),
        (
            "col",
            key.rsplit(':').next().unwrap_or_default().to_string(),
        ),
        ("iter", iter.to_string()),
    ]
}

/// Ladder boundaries (ns) zipped with their cumulative counts into the
/// `(upper bound seconds, count)` pairs the family model stores.
fn classic_pairs(ladder: &[u64], counts: Vec<u64>) -> Vec<(f64, u64)> {
    ladder
        .iter()
        .zip(counts)
        .map(|(&boundary_ns, cumulative)| (seconds(boundary_ns), cumulative))
        .collect()
}

/// f64 `Display` never uses scientific notation, which the exposition format
/// tolerates but some parsers of `le` label values do not.
fn seconds(ns: u64) -> f64 {
    ns as f64 / 1e9
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
        accepts_protobuf, check_auth_with_bearer, query_label, seconds,
    };

    #[test]
    fn call_site_labels_mirror_entry_identity() {
        use crate::prometheus_server::call_site_labels;
        let a = call_site_labels("src/app.rs:10:5", "src/app.rs:10", None, 0);
        assert_eq!(a[0], ("source", "src/app.rs:10".to_string()));
        assert_eq!(a[2], ("col", "5".to_string()));
        assert_eq!(a[3], ("iter", "0".to_string()));

        // Same-line call sites differ by column, repeated instantiations by
        // iter, and the user label stays verbatim (no suffix encoding that
        // could alias two entries).
        let b = call_site_labels("src/app.rs:10:30", "src/app.rs:10", None, 0);
        assert_ne!(a, b);
        let worker_2 = call_site_labels("src/app.rs:10:5", "src/app.rs:10", Some("worker-2"), 0);
        let worker_iter = call_site_labels("src/app.rs:10:5", "src/app.rs:10", Some("worker"), 1);
        assert_ne!(worker_2, worker_iter);
        assert_eq!(worker_iter[1], ("label", "worker".to_string()));
    }

    #[test]
    fn query_label_truncation_stays_unique() {
        assert_eq!(query_label("SELECT 1", 100), "SELECT 1");

        let a = query_label(&format!("SELECT {}", "a".repeat(200)), 50);
        let b = query_label(&format!("SELECT {}", "a".repeat(201)), 50);
        assert_eq!(a.chars().count(), 50 + 3 + 16);
        assert_ne!(a, b, "shared-prefix long queries must stay distinct");
        assert_eq!(a[..50], b[..50]);
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
