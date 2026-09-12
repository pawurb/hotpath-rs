#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::process::{Command, Output};

    use hotpath::json::JsonReport;
    use hotpath::{parse_bytes, parse_duration};

    fn run_with_features(features: &str) -> Output {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "basic_all_features",
            "--features",
            features,
        ])
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env_remove("HOTPATH_UPLOAD")
        .env_remove("HOTPATH_ALLOC_METRIC")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn parse_report(output: &Output) -> JsonReport {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
    }

    /// `(unit, decimals)` of a formatted duration such as `1.004999 ms`.
    fn duration_shape(s: &str) -> (&str, usize) {
        let (number, unit) = s
            .rsplit_once(' ')
            .unwrap_or_else(|| panic!("no unit in {s:?}"));
        let decimals = number.rsplit_once('.').map_or(0, |(_, frac)| frac.len());
        (unit, decimals)
    }

    fn assert_exact_duration(s: &str, what: &str) {
        let expected = match duration_shape(s) {
            ("ns", d) => d == 0,
            ("µs", d) => d == 3,
            ("ms", d) => d == 6,
            ("s", d) => d == 9,
            _ => false,
        };
        assert!(expected, "{what}: {s:?} is not in the exact cloud format");
        assert!(parse_duration(s).is_some(), "{what}: {s:?} does not parse");
    }

    fn assert_display_duration(s: &str, what: &str) {
        let expected = match duration_shape(s) {
            ("ns", d) => d == 0,
            ("µs" | "ms" | "s", d) => d == 2,
            _ => false,
        };
        assert!(expected, "{what}: {s:?} is not in the display format");
    }

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-alloc,hotpath-cloud
    #[test]
    fn cloud_report_renders_exact_values() {
        let report = parse_report(&run_with_features("hotpath,hotpath-alloc,hotpath-cloud"));

        let timing = report.functions_timing.expect("functions_timing section");
        assert!(!timing.data.is_empty());
        assert_exact_duration(&timing.time_elapsed, "time_elapsed");
        for entry in &timing.data {
            assert_exact_duration(&entry.avg, &format!("{} avg", entry.name));
            assert_exact_duration(&entry.total, &format!("{} total", entry.name));
            for (key, value) in &entry.percentiles {
                assert_exact_duration(value, &format!("{} {key}", entry.name));
            }
            // The exact string must carry the value the guard summed, not a
            // rounded one: avg * sampled_calls lands on total exactly only
            // when both survived the formatting.
            if entry.sampled_calls == entry.calls && entry.calls > 0 {
                let avg = parse_duration(&entry.avg).unwrap();
                let total = parse_duration(&entry.total).unwrap();
                assert!(
                    total - avg * entry.calls < entry.calls,
                    "{}: total {total} is not avg {avg} * calls {} within integer division",
                    entry.name,
                    entry.calls
                );
            }
        }

        let alloc = report.functions_alloc.expect("functions_alloc section");
        let total_allocated = alloc.total_allocated.as_deref().expect("total_allocated");
        assert!(
            total_allocated.ends_with(" B"),
            "total_allocated {total_allocated:?} is not an exact byte count"
        );
        let sync_entries: Vec<_> = alloc.data.iter().filter(|e| e.avg != "N/A").collect();
        assert!(!sync_entries.is_empty(), "no sync alloc entries in report");
        for entry in sync_entries {
            for (what, value) in [("avg", &entry.avg), ("total", &entry.total)] {
                assert!(
                    value.ends_with(" B"),
                    "{} {what}: {value:?} is not an exact byte count",
                    entry.name
                );
                assert!(
                    parse_bytes(value).is_some(),
                    "{} {what}: {value:?} does not parse",
                    entry.name
                );
            }
        }

        let mutexes = report.mutexes.expect("mutexes section");
        for entry in &mutexes.data {
            assert_exact_duration(&entry.wait_avg, &format!("{} wait_avg", entry.label));
            assert_exact_duration(&entry.acquire_avg, &format!("{} acquire_avg", entry.label));
        }
        let channels = report.channels.expect("channels section");
        for entry in &channels.data {
            let proc_avg = entry.proc_avg.as_deref().expect("proc_avg");
            assert_exact_duration(proc_avg, &format!("{} proc_avg", entry.label));
        }
    }

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-alloc
    #[test]
    fn display_format_kept_without_the_cloud_feature() {
        let report = parse_report(&run_with_features("hotpath,hotpath-alloc"));

        let timing = report.functions_timing.expect("functions_timing section");
        assert!(!timing.data.is_empty());
        for entry in &timing.data {
            assert_display_duration(&entry.avg, &format!("{} avg", entry.name));
            assert_display_duration(&entry.total, &format!("{} total", entry.name));
        }

        let alloc = report.functions_alloc.expect("functions_alloc section");
        let big = alloc
            .data
            .iter()
            .find(|e| e.name.ends_with("::main"))
            .expect("main alloc entry");
        assert!(
            big.total.ends_with(" KB") || big.total.ends_with(" MB"),
            "main total {:?} should use a rounded unit in display mode",
            big.total
        );
    }
}
