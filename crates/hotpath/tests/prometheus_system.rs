#[cfg(all(test, feature = "hotpath-prometheus"))]
pub mod tests {
    use std::process::Command;
    use std::{thread::sleep, time::Duration};

    const METRICS_PORT: &str = "6779";
    const PROMETHEUS_PORT: &str = "6780";

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

    fn parse_line(line: &str) -> Option<(&str, &str, &str)> {
        let (series, value) = line.rsplit_once(' ')?;
        match series.split_once('{') {
            Some((name, rest)) => Some((name, rest.strip_suffix('}')?, value)),
            None => Some((series, "", value)),
        }
    }

    /// Value of the first `family` series whose labels contain every needle.
    fn series_value(body: &str, family: &str, needles: &[&str]) -> f64 {
        body.lines()
            .find_map(|l| {
                parse_line(l).filter(|(name, labels, _)| {
                    *name == family && needles.iter().all(|n| labels.contains(n))
                })
            })
            .map(|(_, _, value)| value.parse().unwrap())
            .unwrap_or_else(|| panic!("{family} series matching {needles:?} missing"))
    }

    // cargo run -p test-all-features --example prometheus_system --features hotpath,hotpath-prometheus
    #[test]
    fn test_threads_tokio_gauge_families() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-all-features",
                "--example",
                "prometheus_system",
                "--features",
                "hotpath,hotpath-prometheus",
            ])
            .env("HOTPATH_METRICS_PORT", METRICS_PORT)
            .env("HOTPATH_PROMETHEUS_PORT", PROMETHEUS_PORT)
            .env("TEST_SLEEP_SECONDS", "20")
            .spawn()
            .expect("Failed to spawn command");

        // Wait for one tokio runtime sample (1s interval), two thread monitor
        // samples (cpu_percent needs a delta between samples, so the
        // cpu_percent families are absent from the first one) covering the
        // named busy thread, and the gauges.
        let mut scrape = None;
        for _attempt in 0..80 {
            sleep(Duration::from_millis(750));
            if let Ok((200, body)) = get("/metrics") {
                if body.contains("hotpath_tokio_workers")
                    && body.contains("name=\"hp-busy-worker\"")
                    && body.contains("hotpath_thread_cpu_percent_max")
                    && body.contains("hotpath_gauge{")
                {
                    scrape = Some(body);
                    break;
                }
            }
        }
        let Some(body) = scrape else {
            let _ = child.kill();
            panic!("Prometheus server did not serve system metrics on port {PROMETHEUS_PORT}");
        };

        let result = std::panic::catch_unwind(|| {
            for family in [
                "hotpath_threads",
                "hotpath_thread_cpu_seconds_total",
                "hotpath_thread_cpu_percent_max",
                "hotpath_tokio_workers",
                "hotpath_tokio_alive_tasks",
                "hotpath_tokio_global_queue_depth",
                "hotpath_tokio_worker_parks_total",
                "hotpath_tokio_worker_busy_seconds_total",
                "hotpath_gauge",
                "hotpath_gauge_min",
                "hotpath_gauge_max",
                "hotpath_gauge_updates_total",
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

            assert_eq!(series_value(&body, "hotpath_tokio_workers", &[]), 2.0);
            assert_eq!(
                series_value(&body, "hotpath_gauge", &["key=\"test-gauge\""]),
                42.0
            );
            assert_eq!(
                series_value(&body, "hotpath_gauge", &["key=\"queue-depth\""]),
                12.0
            );
            assert_eq!(
                series_value(
                    &body,
                    "hotpath_gauge_updates_total",
                    &["key=\"queue-depth\""]
                ),
                3.0
            );

            // The busy-spinning named thread accumulated user CPU time.
            assert!(
                series_value(
                    &body,
                    "hotpath_thread_cpu_seconds_total",
                    &["name=\"hp-busy-worker\"", "mode=\"user\""]
                ) > 0.0
            );
            assert!(series_value(&body, "hotpath_threads", &[]) >= 3.0);
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
