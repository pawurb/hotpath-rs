#[cfg(all(test, feature = "hotpath-prometheus"))]
pub mod tests {
    use std::collections::HashMap;
    use std::process::Command;
    use std::{thread::sleep, time::Duration};

    const METRICS_PORT: &str = "6785";
    const PROMETHEUS_PORT: &str = "6786";

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

    // cargo run -p test-io --example basic_io_sync --features hotpath,hotpath-prometheus
    #[test]
    fn test_io_families() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-io",
                "--example",
                "basic_io_sync",
                "--features",
                "hotpath,hotpath-prometheus",
            ])
            .env("HOTPATH_METRICS_PORT", METRICS_PORT)
            .env("HOTPATH_PROMETHEUS_PORT", PROMETHEUS_PORT)
            .env("TEST_SLEEP_SECONDS", "15")
            .spawn()
            .expect("Failed to spawn command");

        // The `iter = true` wrappers are created last, so their presence means
        // the whole deterministic workload has been swept.
        let mut scrape = None;
        for _attempt in 0..80 {
            sleep(Duration::from_millis(750));
            if let Ok((200, body)) = get("/metrics") {
                if body.contains("label=\"itered\"") {
                    scrape = Some(body);
                    break;
                }
            }
        }
        let Some(body) = scrape else {
            let _ = child.kill();
            panic!("Prometheus server did not serve io metrics on port {PROMETHEUS_PORT}");
        };

        let result = std::panic::catch_unwind(|| {
            for family in [
                "hotpath_io_ops_total",
                "hotpath_io_bytes_total",
                "hotpath_io_sampled_bytes_total",
                "hotpath_io_errors_total",
                "hotpath_io_op_seconds",
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

            assert_histogram_family(&body, "hotpath_io_op_seconds");

            // The failing reader errors on read, the flush-fail writer on flush.
            assert!(
                series_value(
                    &body,
                    "hotpath_io_errors_total",
                    &["label=\"failing\"", "op=\"read\""]
                ) >= 1.0
            );
            assert!(
                series_value(
                    &body,
                    "hotpath_io_errors_total",
                    &["label=\"flush-fail\"", "op=\"flush\""]
                ) >= 1.0
            );

            // `iter = true` wrappers get one entry per instance: two series
            // distinguished by the iter label.
            for iter in ["iter=\"0\"", "iter=\"1\""] {
                assert_eq!(
                    series_value(
                        &body,
                        "hotpath_io_ops_total",
                        &["label=\"itered\"", "op=\"read\"", iter]
                    ),
                    1.0
                );
            }
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
