#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    use hotpath::json::{JsonFunctionEntry, JsonReport};

    /// Runs the deterministic event-sampling example and parses the JSON
    /// report. Clears sampling env vars first so the host environment cannot
    /// leak in.
    fn run_example_with_features(features: &str, envs: &[(&str, &str)]) -> JsonReport {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "event_sampling",
            "--features",
            features,
        ]);
        for name in [
            "HOTPATH_TIME_SAMPLING_RATE",
            "HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE",
            "HOTPATH_FUNCTIONS_EVENT_SAMPLING_RATE",
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

    // cargo run -p test-all-features --example event_sampling --features hotpath (json)
    #[test]
    fn test_no_event_sampling_is_passthrough() {
        let report = run_example(&[]);

        assert!(report.event_sampling.is_none());
        for name in ["::work", "async_work"] {
            let entry = function_entry(&report, name);
            assert_eq!(entry.calls, 10, "calls for {name}");
            assert_eq!(entry.sampled_calls, 10, "sampled_calls for {name}");
            assert!(!entry.event_sampled, "event_sampled for {name}");
        }
    }

    // The whole workload runs on one thread, so rate 0.5 keeps exactly the
    // even counter values: 5 of 10 calls per function, scaled back to 10.
    // The async decision is made once per call, not per poll - a per-poll
    // decision would advance the counter more than once per call and the
    // scaled count would not land back on the exact call count.
    #[test]
    fn test_rate_half_scales_counts() {
        let report = run_example(&[("HOTPATH_FUNCTIONS_EVENT_SAMPLING_RATE", "0.5")]);

        let rates = report.event_sampling.as_ref().expect("No event_sampling");
        assert_eq!(rates.get("functions"), Some(&0.5));

        for name in ["::work", "async_work"] {
            let entry = function_entry(&report, name);
            assert_eq!(entry.calls, 10, "scaled calls for {name}");
            assert_eq!(entry.sampled_calls, 5, "kept calls for {name}");
            assert!(entry.event_sampled, "event_sampled for {name}");
            assert_ne!(entry.avg, "-");
            assert_ne!(entry.total, "-");
        }

        // The wrapper guard is exempt from event sampling.
        let main = function_entry(&report, "main");
        assert_eq!(main.calls, 1);
        assert!(!main.event_sampled);
    }

    // Rate 0 skips every non-wrapper call entirely: no entries at all, like
    // a HOTPATH_FOCUS matching nothing.
    #[test]
    fn test_rate_zero_skips_everything() {
        let report = run_example(&[("HOTPATH_FUNCTIONS_EVENT_SAMPLING_RATE", "0")]);

        let functions = report
            .functions_timing
            .as_ref()
            .expect("No functions_timing section");
        assert!(
            !functions.data.iter().any(|f| f.name.contains("work")),
            "skipped functions must not appear in the report"
        );

        // The wrapper guard stays measured.
        let main = function_entry(&report, "main");
        assert_eq!(main.calls, 1);
        assert_eq!(main.sampled_calls, 1);
    }

    #[test]
    fn test_builder_rate_applies() {
        let report = run_example(&[("TEST_BUILDER_FUNCTIONS_EVENT_SAMPLING_RATE", "0.5")]);

        let work = function_entry(&report, "::work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 5);
        assert!(work.event_sampled);
    }

    #[test]
    fn test_env_beats_builder() {
        let report = run_example(&[
            ("TEST_BUILDER_FUNCTIONS_EVENT_SAMPLING_RATE", "0"),
            ("HOTPATH_FUNCTIONS_EVENT_SAMPLING_RATE", "1.0"),
        ]);

        let work = function_entry(&report, "::work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 10);
        assert!(!work.event_sampled);
    }

    // Event sampling decides whether the call exists at all, then time
    // sampling decides whether the kept call reads the clock: counts stay
    // scaled while durations go count-only.
    #[test]
    fn test_composes_with_time_sampling() {
        let report = run_example(&[
            ("HOTPATH_FUNCTIONS_EVENT_SAMPLING_RATE", "0.5"),
            ("HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE", "0"),
        ]);

        let work = function_entry(&report, "::work");
        assert_eq!(work.calls, 10);
        assert_eq!(work.sampled_calls, 0);
        assert!(work.event_sampled);
        assert_eq!(work.avg, "-");
        assert_eq!(work.total, "-");
    }

    // `work` allocates exactly 1 KB per call, so the scaled allocation total
    // of the 1-in-2 subset must equal the exact unsampled total.
    #[test]
    fn test_alloc_scaled_totals() {
        let exact = run_example_with_features("hotpath,hotpath-alloc", &[]);
        let sampled = run_example_with_features(
            "hotpath,hotpath-alloc",
            &[("HOTPATH_FUNCTIONS_EVENT_SAMPLING_RATE", "0.5")],
        );

        let alloc_entry = |report: &JsonReport| -> JsonFunctionEntry {
            report
                .functions_alloc
                .as_ref()
                .expect("No functions_alloc section")
                .data
                .iter()
                .find(|f| f.name.contains("work") && !f.name.contains("async"))
                .expect("No `work` entry in functions_alloc")
                .clone()
        };

        let exact_work = alloc_entry(&exact);
        let sampled_work = alloc_entry(&sampled);

        assert_eq!(exact_work.calls, 10);
        assert!(!exact_work.event_sampled);
        assert_eq!(sampled_work.calls, 10);
        assert_eq!(sampled_work.sampled_calls, 5);
        assert!(sampled_work.event_sampled);
        assert_eq!(sampled_work.total, exact_work.total, "scaled bytes total");
        assert_eq!(sampled_work.avg, exact_work.avg, "per-call avg unscaled");
    }
}
