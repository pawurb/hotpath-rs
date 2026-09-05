#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::process::Command;

    use hotpath::json::{JsonFunctionsList, JsonReport};

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-alloc,hotpath-cloud
    fn run_all_features(upload: bool, env: &[(&str, &str)]) -> JsonReport {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "basic_all_features",
            "--features",
            "hotpath,hotpath-alloc,hotpath-cloud",
        ])
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env("HOTPATH_REPORT", "functions-timing,functions-alloc")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
        .env_remove("HOTPATH_LIMIT")
        .env_remove("HOTPATH_FUNCTIONS_LIMIT")
        .env_remove("HOTPATH_UPLOAD_LIMIT");
        if upload {
            cmd.env("HOTPATH_UPLOAD", "1");
        } else {
            cmd.env_remove("HOTPATH_UPLOAD");
        }
        for (key, value) in env {
            cmd.env(key, value);
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

    fn sections(report: JsonReport) -> [(&'static str, JsonFunctionsList); 2] {
        [
            (
                "functions_timing",
                report
                    .functions_timing
                    .expect("No functions_timing section in report"),
            ),
            (
                "functions_alloc",
                report
                    .functions_alloc
                    .expect("No functions_alloc section in report"),
            ),
        ]
    }

    #[test]
    fn display_limit_truncates_and_reports_total_count() {
        let report = run_all_features(false, &[("HOTPATH_FUNCTIONS_LIMIT", "2")]);
        for (name, list) in sections(report) {
            assert_eq!(list.data.len(), 2, "{name}");
            assert!(
                list.total_count > list.data.len(),
                "{name}: total_count {} should exceed data.len() {}",
                list.total_count,
                list.data.len()
            );
        }
    }

    #[test]
    fn upload_ignores_display_limit() {
        let report = run_all_features(true, &[("HOTPATH_FUNCTIONS_LIMIT", "2")]);
        for (name, list) in sections(report) {
            assert!(list.data.len() > 2, "{name}");
            assert_eq!(list.data.len(), list.total_count, "{name}");
        }
    }

    #[test]
    fn upload_limit_caps_uploaded_functions() {
        let report = run_all_features(true, &[("HOTPATH_UPLOAD_LIMIT", "3")]);
        for (name, list) in sections(report) {
            assert_eq!(list.data.len(), 3, "{name}");
            assert!(
                list.total_count > 3,
                "{name}: total_count {}",
                list.total_count
            );
        }
    }

    #[test]
    fn upload_limit_wins_over_display_limit() {
        let report = run_all_features(
            true,
            &[
                ("HOTPATH_FUNCTIONS_LIMIT", "1"),
                ("HOTPATH_UPLOAD_LIMIT", "0"),
            ],
        );
        for (name, list) in sections(report) {
            assert!(list.data.len() > 1, "{name}");
            assert_eq!(list.data.len(), list.total_count, "{name}");
        }
    }
}
