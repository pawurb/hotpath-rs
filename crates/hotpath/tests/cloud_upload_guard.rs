#[cfg(all(test, feature = "hotpath"))]
mod tests {
    //! The upload's verdict as a CI guard, against a mock hotpath.rs that also
    //! mints the OIDC token: `HOTPATH_UPLOAD_FAIL_ON_REGRESSION` decides the
    //! exit code and `HOTPATH_UPLOAD_RESPONSE_PATH` receives the response body.
    //!
    //! cargo test --features hotpath --test cloud_upload_guard -- --nocapture --test-threads=1

    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use hotpath::json::cloud_api::UploadCreated;
    use hotpath::json::JsonReport;
    use mockito::{Matcher, Server, ServerGuard};

    const REGRESSED_BODY: &str = r#"{"id":"r1","repository":"pawurb/hotpath-rs","benchmark":"guard-test","baseline":"r0","comment":{"url":"https://github.com/c/1"},"verdict":{"judged":true,"regressed":true,"regressions":2,"improvements":0,"budgets_broken":1},"dashboard_url":"https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/guard-test/reports/r1/diff"}"#;

    /// A server that mints a token and answers the upload with `status` and `body`.
    fn mock_server(status: usize, body: &str) -> (ServerGuard, mockito::Mock) {
        let mut server = Server::new();
        server
            .mock("GET", "/token")
            .match_query(Matcher::UrlEncoded("audience".into(), "hotpath.rs".into()))
            .with_status(200)
            .with_body(r#"{"value":"oidc-token"}"#)
            .create();
        let upload = server
            .mock("POST", "/api/v1/reports")
            .match_query(Matcher::UrlEncoded("benchmark".into(), "guard-test".into()))
            .match_header("authorization", "Bearer oidc-token")
            .with_status(status)
            .with_body(body)
            .create();
        (server, upload)
    }

    fn scratch_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hotpath_cloud_upload_guard_{test}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-cloud
    fn run_upload(server: &ServerGuard, dir: &Path, env: &[(&str, &str)]) -> Output {
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
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env("HOTPATH_OUTPUT_PATH", dir.join("report.json"))
        .env("HOTPATH_BENCHMARK", "guard-test")
        .env("HOTPATH_UPLOAD", "1")
        .env("HOTPATH_UPLOAD_URL", server.url())
        .env("HOTPATH_UPLOAD_RESPONSE_PATH", dir.join("response.json"))
        .env(
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            format!("{}/token", server.url()),
        )
        .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "request-token")
        .env_remove("HOTPATH_UPLOAD_STRICT")
        .env_remove("HOTPATH_UPLOAD_FAIL_ON_REGRESSION")
        // Under Actions the child would annotate and summarize the test's own job.
        .env_remove("GITHUB_ACTIONS")
        .env_remove("GITHUB_STEP_SUMMARY");
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.output().expect("Failed to execute command")
    }

    fn assert_local_report(dir: &Path) {
        let body =
            std::fs::read_to_string(dir.join("report.json")).expect("report file was not written");
        let report: JsonReport = serde_json::from_str(&body).expect("file is not a JSON report");
        assert!(report.functions_timing.is_some());
    }

    #[test]
    fn regressed_verdict_fails_the_job_when_asked() {
        let dir = scratch_dir("regressed");
        let (server, upload) = mock_server(201, REGRESSED_BODY);

        let output = run_upload(&server, &dir, &[("HOTPATH_UPLOAD_FAIL_ON_REGRESSION", "1")]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        upload.assert();
        assert_eq!(output.status.code(), Some(1), "stderr:\n{stderr}");
        assert!(
            stderr.contains(
                "; verdict: 2 regressions, 1 budget broken, failing the job (HOTPATH_UPLOAD_FAIL_ON_REGRESSION)"
            ),
            "stderr:\n{stderr}"
        );
        assert_local_report(&dir);
        let response = std::fs::read_to_string(dir.join("response.json"))
            .expect("response file was not written");
        assert_eq!(
            serde_json::from_str::<UploadCreated>(&response).unwrap(),
            serde_json::from_str::<UploadCreated>(REGRESSED_BODY).unwrap()
        );

        // Without the switch the same answer is a warning and the job passes.
        let output = run_upload(&server, &dir, &[("HOTPATH_UPLOAD_STRICT", "1")]);
        assert!(
            output.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(dir.join("response.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_upload_never_fails_through_the_switch() {
        let dir = scratch_dir("failed");
        let stale = dir.join("response.json");
        std::fs::write(&stale, REGRESSED_BODY).unwrap();
        let (server, upload) = mock_server(500, r#"{"error":"database error","code":"internal"}"#);

        let output = run_upload(&server, &dir, &[("HOTPATH_UPLOAD_FAIL_ON_REGRESSION", "1")]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        upload.assert();
        assert!(output.status.success(), "stderr:\n{stderr}");
        assert!(
            stderr.contains("hotpath: upload failed: database error (HTTP 500"),
            "stderr:\n{stderr}"
        );
        assert_local_report(&dir);
        assert!(!stale.exists(), "a stale verdict was left behind");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
