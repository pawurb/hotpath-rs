#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use hotpath::json::{JsonLocation, JsonReport};
    use std::process::Command;

    const EXAMPLE_FILE: &str = "crates/test-tokio-async/examples/locations.rs";

    fn run_example(source_root: Option<&str>) -> JsonReport {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-tokio-async",
            "--example",
            "locations",
            "--features",
            "hotpath",
        ]);
        if let Some(root) = source_root {
            cmd.env("HOTPATH_SOURCE_ROOT", root);
        }
        let output = cmd.output().expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
    }

    /// 1-indexed line of the first line containing `needle` in the example
    /// source, so assertions track the file without hardcoding line numbers.
    fn example_line_of(needle: &str) -> u32 {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test-tokio-async/examples/locations.rs"
        );
        let source = std::fs::read_to_string(path).expect("example source readable");
        source
            .lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not found in example")) as u32
            + 1
    }

    fn assert_example_location(location: &JsonLocation, context: &str) {
        assert_eq!(location.file, EXAMPLE_FILE, "{context}");
        assert!(location.line > 0, "{context}: line must be set");
        assert!(location.column > 0, "{context}: column must be set");
    }

    #[test]
    fn test_locations_report() {
        let report = run_example(None);

        // Functions: plain, labeled, measure_all method, measure_block!, and
        // the #[hotpath::main] wrapper all carry a location.
        let functions = report.functions_timing.expect("functions_timing section");
        let entry = |name: &str| {
            functions
                .data
                .iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("no entry named {name}"))
        };

        let plain = entry("locations::plain_function");
        let location = plain.location.as_ref().expect("plain_function location");
        assert_example_location(location, "plain_function");
        assert_eq!(location.line, example_line_of("fn plain_function"));

        let labeled = entry("custom_label_fn");
        let location = labeled.location.as_ref().expect("labeled location");
        assert_example_location(location, "labeled_function");
        assert_eq!(location.line, example_line_of("fn labeled_function"));

        let method = entry("locations::Worker::run");
        let location = method.location.as_ref().expect("Worker::run location");
        assert_example_location(location, "Worker::run");
        assert_eq!(location.line, example_line_of("fn run"));

        let block = entry("locations_block");
        let location = block.location.as_ref().expect("measure_block location");
        assert_example_location(location, "measure_block");
        assert_eq!(location.line, example_line_of("measure_block!"));

        let main_entry = entry("locations::main");
        let location = main_entry.location.as_ref().expect("main location");
        assert_example_location(location, "main");

        // Resource entries: location matches the call line and the `source`
        // string stays the `file:line` form (identity regression guard).
        let mutexes = report.mutexes.expect("mutexes section");
        let mutex = &mutexes.data[0];
        let location = mutex.location.as_ref().expect("mutex location");
        assert_example_location(location, "mutex");
        assert_eq!(location.line, example_line_of("hotpath::mutex!"));
        assert_eq!(
            mutex.source,
            format!("{}:{}", location.file, location.line),
            "mutex source must stay file:line"
        );

        let channels = report.channels.expect("channels section");
        let channel = &channels.data[0];
        let location = channel.location.as_ref().expect("channel location");
        assert_example_location(location, "channel");
        assert_eq!(location.line, example_line_of("hotpath::channel!"));
        assert_eq!(
            channel.source,
            format!("{}:{}", location.file, location.line),
            "channel source must stay file:line"
        );

        // future! ids are `file:line:column` like every other resource macro;
        // the displayed source strips the column back to `file:line`.
        let futures = report.futures.expect("futures section");
        let future = &futures.data[0];
        let location = future.location.as_ref().expect("future location");
        assert_example_location(location, "future");
        assert_eq!(location.line, example_line_of("hotpath::future!"));
        assert_eq!(
            future.source,
            format!("{}:{}", location.file, location.line),
            "future source must stay file:line"
        );

        // meta: environment info plus a source_root derived from the git
        // root. The child inherits this test's working directory (the crate
        // dir), so the derived prefix is the crate dir relative to the repo
        // root.
        let meta = report.meta.expect("meta object");
        assert!(!meta.rustc.is_empty(), "rustc version");
        assert!(meta.os.contains('-'), "os is <os>-<arch>: {}", meta.os);
        assert!(
            meta.created_at.contains('T') && meta.created_at.ends_with('Z'),
            "created_at is RFC 3339 UTC: {}",
            meta.created_at
        );
        assert_eq!(meta.source_root.as_deref(), Some("crates/hotpath"));
    }

    #[test]
    fn test_source_root_env_override() {
        let report = run_example(Some("crates/test-tokio-async"));
        let meta = report.meta.expect("meta object");
        assert_eq!(meta.source_root.as_deref(), Some("crates/test-tokio-async"));
    }

    /// Reports written before `meta`/`location` existed still deserialize.
    #[test]
    fn test_old_report_deserializes() {
        let old = r#"{
            "type": "hotpath_report",
            "version": "0.20.0",
            "functions_timing": {
                "profiling_mode": "timing",
                "time_elapsed": "1s",
                "total_elapsed_ns": 1,
                "description": "d",
                "caller_name": "main",
                "percentiles": [95.0],
                "data": [{
                    "id": 1,
                    "name": "f",
                    "calls": 1,
                    "avg": "1ms",
                    "p95": "1ms",
                    "total": "1ms",
                    "percent_total": "100.00%"
                }]
            }
        }"#;
        let report: JsonReport = serde_json::from_str(old).expect("old report deserializes");
        assert!(report.meta.is_none());
        let functions = report.functions_timing.unwrap();
        assert!(functions.data[0].location.is_none());
    }
}
