#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use hotpath::json::JsonReport;

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-cloud
    fn run_all_features(payload: Option<&Path>, env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "basic_all_features",
            "--features",
            "hotpath,hotpath-cloud",
        ])
        .env("HOTPATH_REPORT", "functions-timing")
        // A table on stdout: the payload is independent of the display format.
        .env("HOTPATH_OUTPUT_FORMAT", "table")
        .env("HOTPATH_BENCHMARK", "fork-test")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
        .env_remove("HOTPATH_UPLOAD")
        .env_remove("HOTPATH_UPLOAD_LIMIT");
        match payload {
            Some(path) => cmd.env("HOTPATH_UPLOAD_PATH", path),
            None => cmd.env_remove("HOTPATH_UPLOAD_PATH"),
        };
        for (key, value) in env {
            cmd.env(key, value);
        }
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn payload_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn payload_is_written_at_upload_fidelity_without_uploading() {
        let path = payload_path("hotpath_cloud_payload_test.json");
        let output = run_all_features(Some(&path), &[("HOTPATH_LIMIT", "1")]);

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("wrote upload payload to {}", path.display())),
            "no payload line on stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("upload skipped"),
            "nothing should attempt an upload:\n{stderr}"
        );

        let body = std::fs::read_to_string(&path).expect("payload file was not written");
        let report: JsonReport = serde_json::from_str(&body).expect("payload is not a JSON report");

        assert_eq!(report.meta.benchmark.as_deref(), Some("fork-test"));

        let functions = report
            .functions_timing
            .expect("no functions_timing section in payload");
        assert!(
            functions.total_count > 1,
            "example must measure several functions"
        );
        assert_eq!(
            functions.included_count, functions.total_count,
            "payload must ignore HOTPATH_LIMIT"
        );
        assert!(
            functions.data.iter().all(|e| e.histogram.is_some()),
            "payload must carry histograms"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.trim_start().starts_with('{'),
            "stdout should still be the configured table format:\n{stdout}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_payload_without_the_variable() {
        let path = payload_path("hotpath_cloud_payload_unset_test.json");
        let output = run_all_features(None, &[]);

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("upload payload"),
            "nothing should be written:\n{stderr}"
        );
        assert!(!path.exists(), "payload file written without the variable");
    }
}
