#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::collections::HashMap;
    use std::process::Command;
    use std::{thread::sleep, time::Duration};

    const METRICS_PORT: &str = "6791";
    const PROMETHEUS_PORT: &str = "6792";
    const TOKEN: &str = "prom-secret";

    fn get(path: &str, auth: Option<&str>) -> Result<(u16, String, String), ureq::Error> {
        let url = format!("http://localhost:{}{}", PROMETHEUS_PORT, path);
        let mut request = ureq::get(&url).config().http_status_as_error(false).build();
        if let Some(token) = auth {
            request = request.header("Authorization", token);
        }
        let mut response = request.call()?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = response.body_mut().read_to_string()?;
        Ok((status, content_type, body))
    }

    fn assert_histogram_series(body: &str) {
        // function name -> (le, cumulative count) pairs in exposition order
        let mut buckets: HashMap<String, Vec<(f64, u64)>> = HashMap::new();
        let mut counts: HashMap<String, u64> = HashMap::new();

        for line in body.lines().filter(|l| !l.starts_with('#')) {
            if let Some(rest) = line.strip_prefix("hotpath_function_duration_seconds_bucket{") {
                let function = label_value(rest, "function");
                let le = label_value(rest, "le");
                let value: u64 = line.rsplit(' ').next().unwrap().parse().unwrap();
                let le = if le == "+Inf" {
                    f64::INFINITY
                } else {
                    le.parse().unwrap()
                };
                buckets.entry(function).or_default().push((le, value));
            } else if let Some(rest) = line.strip_prefix("hotpath_function_duration_seconds_count{")
            {
                let function = label_value(rest, "function");
                let value: u64 = line.rsplit(' ').next().unwrap().parse().unwrap();
                counts.insert(function, value);
            }
        }

        assert!(!buckets.is_empty(), "no bucket series found");
        for (function, series) in &buckets {
            for pair in series.windows(2) {
                assert!(
                    pair[0].0 < pair[1].0,
                    "{function}: le not strictly increasing: {:?}",
                    pair
                );
                assert!(
                    pair[0].1 <= pair[1].1,
                    "{function}: bucket counts decreasing: {:?}",
                    pair
                );
            }
            let (last_le, last_count) = *series.last().unwrap();
            assert!(
                last_le.is_infinite(),
                "{function}: last bucket must be +Inf"
            );
            assert_eq!(
                last_count, counts[function],
                "{function}: +Inf bucket != _count"
            );
        }
    }

    fn label_value(labels: &str, name: &str) -> String {
        let start = labels.find(&format!("{}=\"", name)).unwrap() + name.len() + 2;
        labels[start..].split('"').next().unwrap().to_string()
    }

    // cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_prometheus_endpoint() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", METRICS_PORT)
            .env("HOTPATH_PROMETHEUS", "true")
            .env("HOTPATH_PROMETHEUS_PORT", PROMETHEUS_PORT)
            .env("HOTPATH_PROMETHEUS_AUTH_TOKEN", TOKEN)
            .env("TEST_SLEEP_SECONDS", "15")
            .spawn()
            .expect("Failed to spawn command");

        let mut ready = false;
        for _attempt in 0..60 {
            sleep(Duration::from_millis(750));
            if let Ok((200, _, body)) = get("/metrics", Some(TOKEN)) {
                if body.contains("hotpath_function_duration_seconds_bucket") {
                    ready = true;
                    break;
                }
            }
        }
        if !ready {
            let _ = child.kill();
            panic!("Prometheus server did not serve metrics on port {PROMETHEUS_PORT}");
        }

        let result = std::panic::catch_unwind(|| {
            let (status, _, _) = get("/metrics", None).expect("request without token");
            assert_eq!(status, 401, "missing token");

            let (status, _, _) = get("/metrics", Some("wrong-token")).expect("wrong token");
            assert_eq!(status, 401, "wrong token");

            let (status, _, _) =
                get("/metrics", Some(&format!("Bearer {}", TOKEN))).expect("bearer token");
            assert_eq!(status, 200, "Bearer-prefixed token");

            let (status, _, _) = get("/unknown", Some(TOKEN)).expect("unknown path");
            assert_eq!(status, 404, "unknown path");

            let (status, content_type, body) = get("/metrics", Some(TOKEN)).expect("scrape");
            assert_eq!(status, 200);
            assert_eq!(content_type, "text/plain; version=0.0.4; charset=utf-8");

            for family in [
                "hotpath_build_info",
                "hotpath_uptime_seconds",
                "hotpath_function_calls_total",
                "hotpath_function_duration_seconds",
            ] {
                assert!(
                    body.contains(&format!("# TYPE {family} ")),
                    "missing family {family}, body:\n{body}"
                );
            }
            assert!(
                body.contains("function=\"basic::sync_function\""),
                "instrumented function missing, body:\n{body}"
            );

            // Every non-comment line is `name{labels} value` or `name value`.
            for line in body
                .lines()
                .filter(|l| !l.starts_with('#') && !l.is_empty())
            {
                let (_series, value) = line.rsplit_once(' ').expect("line has no value");
                assert!(
                    value.parse::<f64>().is_ok(),
                    "unparsable value in line: {line}"
                );
            }

            assert_histogram_series(&body);
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
