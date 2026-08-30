#[cfg(all(test, feature = "hotpath-prometheus"))]
pub mod tests {
    use std::collections::HashMap;
    use std::process::Command;
    use std::{thread::sleep, time::Duration};

    const METRICS_PORT: &str = "6798";
    const PROMETHEUS_PORT: &str = "6799";

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

    fn label_value(labels: &str, name: &str) -> String {
        let start = labels.find(&format!("{}=\"", name)).unwrap() + name.len() + 2;
        labels[start..].split('"').next().unwrap().to_string()
    }

    /// One series line of `family` whose labels contain `needle`, parsed.
    fn series_value(body: &str, family: &str, needle: &str) -> f64 {
        body.lines()
            .find_map(|l| {
                parse_line(l).filter(|(name, labels, _)| *name == family && labels.contains(needle))
            })
            .map(|(_, _, value)| value.parse().unwrap())
            .unwrap_or_else(|| panic!("{family} series matching {needle} missing"))
    }

    /// For one histogram family: `le` strictly increasing with `+Inf` last,
    /// bucket counts non-decreasing, `+Inf` bucket == `_count`, per series.
    fn assert_histogram_family(body: &str, family: &str) {
        let bucket_name = format!("{family}_bucket");
        let count_name = format!("{family}_count");
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

    // cargo run -p test-all-features --example prometheus_flow --features hotpath,hotpath-prometheus
    #[test]
    fn test_locks_channels_streams_families() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-all-features",
                "--example",
                "prometheus_flow",
                "--features",
                "hotpath,hotpath-prometheus",
            ])
            .env("HOTPATH_METRICS_PORT", METRICS_PORT)
            .env("HOTPATH_PROMETHEUS_PORT", PROMETHEUS_PORT)
            .env("TEST_SLEEP_SECONDS", "15")
            .spawn()
            .expect("Failed to spawn command");

        // The workload is deterministic; wait for its final state (50 received
        // messages, 7 stream items) so assertions run past every worker sweep.
        let mut scrape = None;
        for _attempt in 0..80 {
            sleep(Duration::from_millis(750));
            if let Ok((200, body)) = get("/metrics") {
                if body.contains("hotpath_stream_items_total")
                    && body.lines().any(|l| {
                        l.starts_with("hotpath_channel_received_total{") && l.ends_with(" 50")
                    })
                {
                    scrape = Some(body);
                    break;
                }
            }
        }
        let Some(body) = scrape else {
            let _ = child.kill();
            panic!("Prometheus server did not serve flow metrics on port {PROMETHEUS_PORT}");
        };

        let result = std::panic::catch_unwind(|| {
            for family in [
                "hotpath_mutex_acquisitions_total",
                "hotpath_mutex_wait_seconds",
                "hotpath_mutex_acquire_seconds",
                "hotpath_rwlock_acquisitions_total",
                "hotpath_rwlock_wait_seconds",
                "hotpath_rwlock_acquire_seconds",
                "hotpath_channel_sent_total",
                "hotpath_channel_received_total",
                "hotpath_channel_instances",
                "hotpath_channel_closed_instances",
                "hotpath_channel_max_queue_size",
                "hotpath_channel_proc_seconds",
                "hotpath_stream_items_total",
                "hotpath_stream_instances",
                "hotpath_stream_closed_instances",
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
                "hotpath_mutex_wait_seconds",
                "hotpath_mutex_acquire_seconds",
                "hotpath_rwlock_wait_seconds",
                "hotpath_rwlock_acquire_seconds",
                "hotpath_channel_proc_seconds",
            ] {
                assert_histogram_family(&body, family);
            }

            // Deterministic workload counts.
            assert_eq!(
                series_value(&body, "hotpath_channel_sent_total", "label=\"work-queue\""),
                50.0
            );
            assert_eq!(
                series_value(
                    &body,
                    "hotpath_stream_items_total",
                    "label=\"number-stream\""
                ),
                7.0
            );
            assert_eq!(
                series_value(&body, "hotpath_rwlock_acquisitions_total", "op=\"write\""),
                5.0
            );
            assert_eq!(
                series_value(&body, "hotpath_rwlock_acquisitions_total", "op=\"read\""),
                15.0
            );

            // Sampling honesty: the histogram population never exceeds the
            // true acquisition count.
            let acquisitions = series_value(
                &body,
                "hotpath_mutex_acquisitions_total",
                "label=\"counter\"",
            );
            let sampled = series_value(
                &body,
                "hotpath_mutex_wait_seconds_count",
                "label=\"counter\"",
            );
            assert_eq!(acquisitions, 20.0);
            assert!(
                sampled <= acquisitions,
                "sampled {sampled} > acquisitions {acquisitions}"
            );
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
