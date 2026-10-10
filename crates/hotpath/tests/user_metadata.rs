#[path = "common/support.rs"]
mod common;

#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::collections::BTreeMap;

    use crate::common::example::Example;

    // cargo run -p test-smol-async --example user_metadata --features hotpath
    fn example(format: &str, env_metadata: Option<&str>) -> Example {
        let example = Example::new("test-smol-async", "user_metadata")
            .env("HOTPATH_OUTPUT_FORMAT", format)
            .env("HOTPATH_METRICS_SERVER_OFF", "true")
            .env_remove("HOTPATH_USER_METADATA");
        match env_metadata {
            Some(raw) => example.env("HOTPATH_USER_METADATA", raw),
            None => example,
        }
    }

    fn pairs(map: &BTreeMap<String, String>) -> Vec<(&str, &str)> {
        map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    #[test]
    fn builder_metadata_lands_in_json_report() {
        let report = example("json", None).report();
        let metadata = report.user_metadata.expect("user_metadata missing");
        assert_eq!(
            pairs(&metadata),
            vec![("source", "builder"), ("team", "core")]
        );
    }

    #[test]
    fn env_metadata_merges_over_builder_metadata() {
        let report = example("json", Some(" source = env ,commit=abc=def, empty= ")).report();
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
        let stdout = example("table", Some("commit=abc123")).stdout();
        assert!(
            stdout.contains("user_metadata - Custom report metadata."),
            "missing metadata section header:\n{stdout}"
        );
        for (key, value) in [
            ("commit", "abc123"),
            ("source", "builder"),
            ("team", "core"),
        ] {
            assert!(
                stdout.lines().any(|l| l.contains(key) && l.contains(value)),
                "missing row {key}={value}:\n{stdout}"
            );
        }
    }

    #[test]
    fn invalid_env_metadata_panics_at_build() {
        let output = example("json", Some("commit")).output();
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
