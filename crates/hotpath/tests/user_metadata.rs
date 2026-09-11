#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::collections::BTreeMap;
    use std::process::{Command, Output};

    use hotpath::json::JsonReport;

    // cargo run -p test-smol-async --example user_metadata --features hotpath
    fn run_example(format: &str, env_metadata: Option<&str>) -> Output {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-smol-async",
            "--example",
            "user_metadata",
            "--features",
            "hotpath",
        ])
        .env("HOTPATH_OUTPUT_FORMAT", format)
        .env("HOTPATH_METRICS_SERVER_OFF", "true")
        .env_remove("HOTPATH_USER_METADATA");
        if let Some(raw) = env_metadata {
            cmd.env("HOTPATH_USER_METADATA", raw);
        }
        cmd.output().expect("Failed to execute command")
    }

    fn parse_report(output: &Output) -> JsonReport {
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

    fn pairs(map: &BTreeMap<String, String>) -> Vec<(&str, &str)> {
        map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    #[test]
    fn builder_metadata_lands_in_json_report() {
        let report = parse_report(&run_example("json", None));
        let metadata = report.user_metadata.expect("user_metadata missing");
        assert_eq!(
            pairs(&metadata),
            vec![("source", "builder"), ("team", "core")]
        );
    }

    #[test]
    fn env_metadata_merges_over_builder_metadata() {
        let report = parse_report(&run_example(
            "json",
            Some(" source = env ,commit=abc=def, empty= "),
        ));
        let metadata = report.user_metadata.expect("user_metadata missing");
        assert_eq!(
            pairs(&metadata),
            vec![
                ("commit", "abc=def"),
                ("empty", ""),
                ("source", "env"),
                ("team", "core"),
            ]
        );
    }

    #[test]
    fn table_output_renders_metadata_table() {
        let output = run_example("table", Some("commit=abc123"));
        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("user_metadata - Custom report metadata."),
            "missing metadata section header:\n{stdout}"
        );
        for (key, value) in [
            ("commit", "abc123"),
            ("source", "builder"),
            ("team", "core"),
        ] {
            let row = stdout
                .lines()
                .find(|l| l.contains(key) && l.contains(value));
            assert!(row.is_some(), "missing row {key}={value}:\n{stdout}");
        }
    }

    #[test]
    fn invalid_env_metadata_panics_at_build() {
        let output = run_example("json", Some("commit"));
        assert!(
            !output.status.success(),
            "expected the example to fail on malformed HOTPATH_USER_METADATA"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid HOTPATH_USER_METADATA: expected 'key=value', got 'commit'"),
            "unexpected stderr:\n{stderr}"
        );
    }
}
