#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    use hotpath::json::{JsonFunctionEntry, JsonReport};

    /// Runs the deterministic time-sampling example and parses the JSON report.
    /// Clears sampling env vars first so the host environment cannot leak in.
    fn run_example_with_features(features: &str, envs: &[(&str, &str)]) -> JsonReport {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "time_sampling",
            "--features",
            features,
        ]);
        for name in [
            "HOTPATH_TIME_SAMPLING_RATE",
            "HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE",
            "HOTPATH_MUTEXES_TIME_SAMPLING_RATE",
            "HOTPATH_RW_LOCKS_TIME_SAMPLING_RATE",
            "HOTPATH_FUTURES_TIME_SAMPLING_RATE",
            "HOTPATH_CHANNELS_TIME_SAMPLING_RATE",
        ] {
            cmd.env_remove(name);
        }
        cmd.env("HOTPATH_OUTPUT_FORMAT", "json");
        for (name, value) in envs {
            cmd.env(name, value);
        }

        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
    }

    fn run_example(envs: &[(&str, &str)]) -> JsonReport {
        run_example_with_features("hotpath", envs)
    }

    fn function_entry<'a>(report: &'a JsonReport, name: &str) -> &'a JsonFunctionEntry {
        report
            .functions_timing
            .as_ref()
            .expect("No functions_timing section")
            .data
            .iter()
            .find(|f| f.name.contains(name))
            .unwrap_or_else(|| panic!("No `{name}` entry in functions_timing"))
    }

    // cargo run -p test-all-features --example time_sampling --features hotpath (json)
    #[test]
    fn test_no_sampling_is_passthrough() {
        let report = run_example(&[]);

        assert!(report.time_sampling.is_none());
        let work = function_entry(&report, "work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 10);

        let mutex = &report.mutexes.as_ref().expect("No mutexes section").data[0];
        assert_eq!(mutex.count, 10);
        assert_eq!(mutex.sampled_count, 10);

        let channel = &report.channels.as_ref().expect("No channels section").data[0];
        assert_eq!(channel.received_count, 10);
        assert_eq!(channel.proc_sampled_count, Some(10));
    }

    #[test]
    fn test_global_rate_applies_to_all_resources() {
        let report = run_example(&[("HOTPATH_TIME_SAMPLING_RATE", "0.5")]);

        let rates = report.time_sampling.as_ref().expect("No time_sampling");
        for resource in ["functions", "mutexes", "rw_locks", "futures", "channels"] {
            assert_eq!(rates.get(resource), Some(&0.5), "rate for {resource}");
        }

        // Thread-local counter starts at 0, 0 is sampled: indices 0,2,4,6,8.
        let work = function_entry(&report, "work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 5);

        // The wrapper guard is exempt from sampling.
        let main = function_entry(&report, "main");
        assert_eq!(main.sampled_calls, main.calls);

        let mutex = &report.mutexes.as_ref().expect("No mutexes section").data[0];
        assert_eq!(mutex.count, 10);
        assert_eq!(mutex.sampled_count, 5);

        // One shared counter across kinds: 10 reads (5 sampled) then 4 writes
        // at counter values 10..13 (10 and 12 sampled).
        let rw = &report.rw_locks.as_ref().expect("No rw_locks section").data[0];
        assert_eq!(rw.read_count, 10);
        assert_eq!(rw.read_sampled_count, 5);
        assert_eq!(rw.write_count, 4);
        assert_eq!(rw.write_sampled_count, 2);

        // Wrap channel keyed on msg_id: 0,2,4,6,8 sampled.
        let channel = &report.channels.as_ref().expect("No channels section").data[0];
        assert_eq!(channel.sent_count, 10);
        assert_eq!(channel.received_count, 10);
        assert_eq!(channel.proc_sampled_count, Some(5));
        assert_ne!(channel.proc_avg.as_deref(), Some("-"));
    }

    #[test]
    fn test_rate_zero_is_count_only() {
        let report = run_example(&[("HOTPATH_TIME_SAMPLING_RATE", "0")]);

        let work = function_entry(&report, "work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 0);
        assert_eq!(work.avg, "-");
        assert_eq!(work.total, "-");
        assert_eq!(work.percent_total, "-");

        // The wrapper guard stays measured even in count-only mode.
        let main = function_entry(&report, "main");
        assert_eq!(main.calls, 1);
        assert_eq!(main.sampled_calls, 1);

        let mutex = &report.mutexes.as_ref().expect("No mutexes section").data[0];
        assert_eq!(mutex.count, 10);
        assert_eq!(mutex.sampled_count, 0);
        assert_eq!(mutex.wait_avg, "-");

        let channel = &report.channels.as_ref().expect("No channels section").data[0];
        assert_eq!(channel.received_count, 10);
        assert_eq!(channel.proc_sampled_count, Some(0));
        assert_eq!(channel.proc_avg.as_deref(), Some("-"));
    }

    #[test]
    fn test_per_resource_env_beats_global() {
        let report = run_example(&[
            ("HOTPATH_TIME_SAMPLING_RATE", "0"),
            ("HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE", "1.0"),
        ]);

        let work = function_entry(&report, "work");
        assert_eq!(work.sampled_calls, 10);

        let mutex = &report.mutexes.as_ref().expect("No mutexes section").data[0];
        assert_eq!(mutex.sampled_count, 0);
    }

    #[test]
    fn test_builder_rate_applies() {
        let report = run_example(&[("TEST_BUILDER_TIME_SAMPLING_RATE", "0.5")]);

        let work = function_entry(&report, "work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 5);

        let mutex = &report.mutexes.as_ref().expect("No mutexes section").data[0];
        assert_eq!(mutex.sampled_count, 5);
    }

    #[test]
    fn test_env_beats_builder() {
        let report = run_example(&[
            ("TEST_BUILDER_FUNCTIONS_TIME_SAMPLING_RATE", "0"),
            ("HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE", "1.0"),
        ]);

        let work = function_entry(&report, "work");
        assert_eq!(work.sampled_calls, 10);
    }

    // Under hotpath-alloc the guards measure both allocations and time:
    // durations respect the rate while allocation metrics stay exact.
    #[test]
    fn test_alloc_mode_samples_durations_only() {
        let report = run_example_with_features(
            "hotpath,hotpath-alloc",
            &[("HOTPATH_TIME_SAMPLING_RATE", "0.5")],
        );

        let rates = report.time_sampling.as_ref().expect("No time_sampling");
        assert_eq!(rates.get("functions"), Some(&0.5));

        let work = function_entry(&report, "work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 5);
        assert_ne!(work.avg, "-");

        let alloc_work = report
            .functions_alloc
            .as_ref()
            .expect("No functions_alloc section")
            .data
            .iter()
            .find(|f| f.name.contains("work"))
            .expect("No `work` entry in functions_alloc");
        assert_eq!(alloc_work.calls, 10);
        assert_eq!(alloc_work.sampled_calls, 10);
    }

    // Poll counts vary with runtime scheduling, so only count-only mode
    // (rate 0) gives a deterministic futures assertion.
    // cargo run -p test-futures --example basic_futures --features hotpath (json)
    #[test]
    fn test_futures_count_only() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "basic_futures",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .env("HOTPATH_FUTURES_TIME_SAMPLING_RATE", "0")
            .env("HOTPATH_REPORT", "futures")
            .output()
            .expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");

        let futures = report.futures.expect("No futures section");
        assert!(futures.data.iter().any(|e| e.total_polls > 0));
        for entry in &futures.data {
            assert_eq!(entry.sampled_polls, 0, "sampled polls for {}", entry.label);
            assert_eq!(
                entry.total_poll_duration_ns, 0,
                "duration recorded for {}",
                entry.label
            );
        }
    }
}
