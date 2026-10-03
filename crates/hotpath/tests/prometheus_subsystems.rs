#[cfg(all(test, feature = "hotpath-prometheus"))]
pub mod tests {
    use std::collections::HashMap;
    use std::process::Command;
    use std::{thread::sleep, time::Duration};

    const METRICS_PORT: &str = "6793";
    const PROMETHEUS_PORT: &str = "6794";

    fn get(path: &str) -> Result<(u16, String), ureq::Error> {
        let url = format!("http://localhost:{}{}", PROMETHEUS_PORT, path);
        let mut response = ureq::get(&url)
            .config()
            .http_status_as_error(false)
            .build()
            .call()?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string()?;
        Ok((status, body))
    }

    /// Splits a sample line into its label section and value, tolerating `}`
    /// inside label values (route templates like `GET /users/{id}`).
    fn parse_line(line: &str) -> Option<(&str, &str, &str)> {
        let (series, value) = line.rsplit_once(' ')?;
        match series.split_once('{') {
            Some((name, rest)) => Some((name, rest.strip_suffix('}')?, value)),
            None => Some((series, "", value)),
        }
    }

    fn label_value(labels: &str, name: &str) -> String {
        let start = labels.find(&format!("{}=\"", name)).unwrap() + name.len() + 2;
        labels[start..].split('"').next().unwrap().to_string()
    }

    /// For one histogram family: `le` strictly increasing with `+Inf` last,
    /// bucket counts non-decreasing, `+Inf` bucket == `_count`, per series.
    fn assert_histogram_family(body: &str, family: &str) {
        let bucket_name = format!("{family}_bucket");
        let count_name = format!("{family}_count");
        // labels-without-le -> (le, cumulative count) pairs in exposition order
        let mut buckets: HashMap<String, Vec<(f64, u64)>> = HashMap::new();
        let mut counts: HashMap<String, u64> = HashMap::new();

        for line in body.lines().filter(|l| !l.starts_with('#')) {
            let Some((name, labels, value)) = parse_line(line) else {
                continue;
            };
            if name == bucket_name {
                let le = label_value(labels, "le");
                let le = if le == "+Inf" {
                    f64::INFINITY
                } else {
                    le.parse().unwrap()
                };
                let key = labels[..labels.rfind(",le=\"").expect("le must be last")].to_string();
                buckets
                    .entry(key)
                    .or_default()
                    .push((le, value.parse().unwrap()));
            } else if name == count_name {
                counts.insert(labels.to_string(), value.parse().unwrap());
            }
        }

        assert!(!buckets.is_empty(), "{family}: no bucket series found");
        for (series, pairs) in &buckets {
            for pair in pairs.windows(2) {
                assert!(
                    pair[0].0 < pair[1].0,
                    "{family}{{{series}}}: le not strictly increasing: {:?}",
                    pair
                );
                assert!(
                    pair[0].1 <= pair[1].1,
                    "{family}{{{series}}}: bucket counts decreasing: {:?}",
                    pair
                );
            }
            let (last_le, last_count) = *pairs.last().unwrap();
            assert!(
                last_le.is_infinite(),
                "{family}{{{series}}}: last bucket must be +Inf"
            );
            assert_eq!(
                last_count, counts[series],
                "{family}{{{series}}}: +Inf bucket != _count"
            );
        }
    }

    // cargo run -p test-axum --example route_scope --features hotpath,hotpath-prometheus
    #[test]
    fn test_sql_http_server_families() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-axum",
                "--example",
                "route_scope",
                "--features",
                "hotpath,hotpath-prometheus",
            ])
            .env("HOTPATH_METRICS_PORT", METRICS_PORT)
            .env("HOTPATH_PROMETHEUS_PORT", PROMETHEUS_PORT)
            .env("TEST_SLEEP_SECONDS", "15")
            .spawn()
            .expect("Failed to spawn command");

        // Wait for all 3 GET /profiles/{id} requests: sql/http attribution and request
        // completion arrive through different worker queues.
        let mut scrape = None;
        for _attempt in 0..80 {
            sleep(Duration::from_millis(750));
            if let Ok((200, body)) = get("/metrics") {
                if body.contains(
                    "hotpath_server_scoped_requests_total{route=\"GET /profiles/{id}\"} 3",
                ) && body.contains("hotpath_http_duration_seconds_bucket")
                {
                    scrape = Some(body);
                    break;
                }
            }
        }
        let Some(body) = scrape else {
            let _ = child.kill();
            panic!(
                "Prometheus server did not serve sql/http/server metrics on port {PROMETHEUS_PORT}"
            );
        };

        let result = std::panic::catch_unwind(|| {
            for family in [
                "hotpath_sql_queries_total",
                "hotpath_sql_duration_seconds",
                "hotpath_http_requests_total",
                "hotpath_http_errors_total",
                "hotpath_http_duration_seconds",
                "hotpath_server_requests_total",
                "hotpath_server_responses_total",
                "hotpath_server_duration_seconds",
                "hotpath_server_scoped_requests_total",
                "hotpath_server_sql_calls_total",
                "hotpath_server_http_calls_total",
            ] {
                assert!(
                    body.contains(&format!("# TYPE {family} ")),
                    "missing family {family}, body:\n{body}"
                );
            }

            // Every non-comment line is `name{labels} value` or `name value`.
            for line in body
                .lines()
                .filter(|l| !l.starts_with('#') && !l.is_empty())
            {
                let (_, _, value) = parse_line(line).expect("line has no value");
                assert!(
                    value.parse::<f64>().is_ok(),
                    "unparsable value in line: {line}"
                );
            }

            for family in [
                "hotpath_sql_duration_seconds",
                "hotpath_http_duration_seconds",
                "hotpath_server_duration_seconds",
            ] {
                assert_histogram_family(&body, family);
            }

            // SQL series are identified by normalized query text plus the
            // route/source attribution.
            let duration_count = body
                .lines()
                .find(|l| {
                    l.starts_with("hotpath_sql_duration_seconds_count{")
                        && l.contains("query=\"SELECT COUNT(*) FROM users\"")
                })
                .expect("duration series for COUNT query missing");
            let (_, labels, _) = parse_line(duration_count).unwrap();
            assert_eq!(label_value(labels, "route"), "GET /profiles/{id}");
            assert_eq!(label_value(labels, "source"), "route_scope::count_users");

            // The unmatched /missing request produced a 4xx response.
            let has_4xx = body.lines().any(|l| {
                parse_line(l).is_some_and(|(name, labels, value)| {
                    name == "hotpath_server_responses_total"
                        && labels.contains("class=\"4xx\"")
                        && value.parse::<f64>().unwrap_or(0.0) >= 1.0
                })
            });
            assert!(has_4xx, "no 4xx responses counted, body:\n{body}");

            // Route-scoped attribution: GET /profiles/{id} issues 2 SQL
            // queries and 1 outbound HTTP request per request.
            let scoped_value = |name: &str| {
                body.lines()
                    .find_map(|l| {
                        parse_line(l).filter(|(n, labels, _)| {
                            *n == name && label_value(labels, "route") == "GET /profiles/{id}"
                        })
                    })
                    .map(|(_, _, value)| value.parse::<f64>().unwrap())
                    .unwrap_or_else(|| panic!("{name} series for GET /profiles/{{id}} missing"))
            };
            assert_eq!(scoped_value("hotpath_server_scoped_requests_total"), 3.0);
            assert_eq!(scoped_value("hotpath_server_sql_calls_total"), 6.0);
            assert_eq!(scoped_value("hotpath_server_http_calls_total"), 3.0);

            // Timing-mode functions are split by route too: load_user runs
            // under both routes (2 + 3 calls), count_users under one.
            let function_route_value = |name: &str, function: &str, route: &str| {
                body.lines()
                    .find_map(|l| {
                        parse_line(l).filter(|(n, labels, _)| {
                            *n == name
                                && label_value(labels, "function") == function
                                && label_value(labels, "route") == route
                        })
                    })
                    .map(|(_, _, value)| value.parse::<f64>().unwrap())
                    .unwrap_or_else(|| panic!("{name} series for {function} / {route} missing"))
            };
            let calls = "hotpath_function_route_calls_total";
            assert_eq!(
                function_route_value(calls, "route_scope::load_user", "GET /users/{id}"),
                5.0
            );
            assert_eq!(
                function_route_value(calls, "route_scope::load_user", "GET /profiles/{id}"),
                3.0
            );
            assert_eq!(
                function_route_value(calls, "route_scope::count_users", "GET /profiles/{id}"),
                3.0
            );
            assert!(
                function_route_value(
                    "hotpath_function_route_duration_seconds_total",
                    "route_scope::load_user",
                    "GET /profiles/{id}"
                ) > 0.0
            );
            assert!(
                !body.contains("hotpath_function_route_alloc_bytes_total"),
                "alloc route families exported without hotpath-alloc"
            );
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    // cargo run -p test-axum --example route_alloc --features hotpath,hotpath-alloc,hotpath-prometheus
    #[test]
    fn test_server_alloc_families() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-axum",
                "--example",
                "route_alloc",
                "--features",
                "hotpath,hotpath-alloc,hotpath-prometheus",
            ])
            .env("HOTPATH_METRICS_PORT", METRICS_PORT)
            .env("HOTPATH_PROMETHEUS_PORT", PROMETHEUS_PORT)
            .env("TEST_SLEEP_SECONDS", "15")
            .spawn()
            .expect("Failed to spawn command");

        // The example serves exactly 3 GET /big requests.
        let mut scrape = None;
        for _attempt in 0..80 {
            sleep(Duration::from_millis(750));
            if let Ok((200, body)) = get("/metrics") {
                if body.contains("hotpath_server_scoped_requests_total{route=\"GET /big\"} 3") {
                    scrape = Some(body);
                    break;
                }
            }
        }
        let Some(body) = scrape else {
            let _ = child.kill();
            panic!(
                "Prometheus server did not serve server alloc metrics on port {PROMETHEUS_PORT}"
            );
        };

        let result = std::panic::catch_unwind(|| {
            for family in [
                "hotpath_server_alloc_bytes_total",
                "hotpath_server_alloc_count_total",
                "hotpath_server_alloc_bytes",
            ] {
                assert!(
                    body.contains(&format!("# TYPE {family} ")),
                    "missing family {family}, body:\n{body}"
                );
            }
            assert_histogram_family(&body, "hotpath_server_alloc_bytes");

            let route_value = |name: &str, route: &str| {
                body.lines()
                    .find_map(|l| {
                        parse_line(l).filter(|(n, labels, _)| {
                            *n == name && label_value(labels, "route") == route
                        })
                    })
                    .map(|(_, _, value)| value.parse::<f64>().unwrap())
                    .unwrap_or_else(|| panic!("{name} series for {route} missing"))
            };
            // Three 1 MiB bodies, and at least one allocation each.
            let big_bytes = route_value("hotpath_server_alloc_bytes_total", "GET /big");
            assert!(big_bytes >= 3.0 * 1024.0 * 1024.0, "{big_bytes}");
            assert!(route_value("hotpath_server_alloc_count_total", "GET /big") >= 3.0);
            assert_eq!(
                route_value("hotpath_server_alloc_bytes_count", "GET /big"),
                3.0
            );
            assert_eq!(
                route_value("hotpath_server_alloc_bytes_sum", "GET /big"),
                big_bytes
            );
            assert!(route_value("hotpath_server_alloc_bytes_total", "GET /small") < 64.0 * 1024.0);

            // Unmatched requests carry no scope: counters exist at zero, but
            // there is no per-request distribution to export.
            assert_eq!(
                route_value("hotpath_server_alloc_bytes_total", "GET <unmatched>"),
                0.0
            );
            assert!(
                !body.contains("hotpath_server_alloc_bytes_count{route=\"GET <unmatched>\"}"),
                "unscoped route exported a histogram, body:\n{body}"
            );

            // Measured functions are split by the route they ran under.
            let function_route_value = |name: &str| {
                body.lines()
                    .find_map(|l| {
                        parse_line(l).filter(|(n, labels, _)| {
                            *n == name
                                && label_value(labels, "function") == "route_alloc::build_body"
                                && label_value(labels, "route") == "GET /big"
                        })
                    })
                    .map(|(_, _, value)| value.parse::<f64>().unwrap())
                    .unwrap_or_else(|| panic!("{name} series for build_body / GET /big missing"))
            };
            assert_eq!(
                function_route_value("hotpath_function_route_calls_total"),
                3.0
            );
            assert_eq!(
                function_route_value("hotpath_function_route_timed_calls_total"),
                3.0
            );
            assert!(function_route_value("hotpath_function_route_duration_seconds_total") > 0.0);
            assert!(
                function_route_value("hotpath_function_route_alloc_bytes_total")
                    >= 3.0 * 1024.0 * 1024.0
            );
            assert_eq!(
                function_route_value("hotpath_function_route_alloc_count_total"),
                3.0
            );
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
