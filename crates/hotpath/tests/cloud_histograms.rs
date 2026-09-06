#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::process::{Command, Output};

    use base64::Engine;
    use hdrhistogram::serialization::Deserializer;
    use hdrhistogram::Histogram;
    use hotpath::json::{JsonFunctionsList, JsonReport};

    fn run_example(package: &str, example: &str, report: Option<&str>, upload: bool) -> Output {
        run_with_features(package, example, "hotpath,hotpath-cloud", report, upload)
    }

    /// Histograms exist only with the cloud feature compiled in, so absence is
    /// tested against a build without it rather than against `HOTPATH_UPLOAD`.
    fn run_without_cloud(package: &str, example: &str, report: Option<&str>) -> Output {
        run_with_features(package, example, "hotpath", report, false)
    }

    fn run_with_features(
        package: &str,
        example: &str,
        features: &str,
        report: Option<&str>,
        upload: bool,
    ) -> Output {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            package,
            "--example",
            example,
            "--features",
            features,
        ])
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
        if let Some(report) = report {
            cmd.env("HOTPATH_REPORT", report);
        }
        if upload {
            cmd.env("HOTPATH_UPLOAD", "1");
        } else {
            cmd.env_remove("HOTPATH_UPLOAD");
        }
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn parse_report(stdout: &str) -> JsonReport {
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
    }

    fn decode(b64: &str) -> Histogram<u64> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("histogram is not valid base64");
        Deserializer::new()
            .deserialize(&mut &bytes[..])
            .expect("histogram is not a valid HdrHistogram payload")
    }

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-cloud
    fn run_all_features(upload: bool) -> JsonFunctionsList {
        let output = run_example(
            "test-all-features",
            "basic_all_features",
            Some("functions-timing"),
            upload,
        );
        if upload {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("upload skipped: not in GitHub Actions"),
                "Expected upload skip message, got:\n{stderr}"
            );
        }
        parse_report(&String::from_utf8_lossy(&output.stdout))
            .functions_timing
            .expect("No functions_timing section in report")
    }

    #[test]
    fn histograms_attached_when_upload_enabled() {
        let list = run_all_features(true);
        let entry = list
            .data
            .iter()
            .find(|e| e.name.ends_with("sync_function"))
            .expect("sync_function missing from report");
        let b64 = entry
            .histogram
            .as_ref()
            .expect("histogram missing from sync_function entry");

        let hist = decode(b64);
        assert_eq!(hist.len(), entry.sampled_calls);
        assert!(hist.max() > 0);

        for entry in &list.data {
            assert_eq!(
                entry.histogram.is_some(),
                entry.sampled_calls > 0,
                "histogram presence mismatch for {}",
                entry.name
            );
        }
    }

    #[test]
    fn histograms_absent_without_the_cloud_feature() {
        let output = run_without_cloud(
            "test-all-features",
            "basic_all_features",
            Some("functions-timing"),
        );
        let list = parse_report(&String::from_utf8_lossy(&output.stdout))
            .functions_timing
            .expect("No functions_timing section in report");
        assert!(!list.data.is_empty());
        assert!(list.data.iter().all(|e| e.histogram.is_none()));
    }

    // cargo run -p test-axum --example route_scope --features hotpath,hotpath-cloud
    #[test]
    fn server_sql_http_histograms_attached_when_upload_enabled() {
        let output = run_example("test-axum", "route_scope", None, true);
        let report = parse_report(&String::from_utf8_lossy(&output.stdout));

        let server = report.server.expect("No server section in report");
        assert!(!server.data.is_empty());
        for entry in &server.data {
            let b64 = entry
                .histogram
                .as_ref()
                .unwrap_or_else(|| panic!("histogram missing for route {}", entry.route));
            let hist = decode(b64);
            assert_eq!(hist.len(), entry.count, "route {}", entry.route);
        }

        let sql = report.sql.expect("No sql section in report");
        assert!(!sql.data.is_empty());
        for entry in &sql.data {
            let b64 = entry
                .histogram
                .as_ref()
                .unwrap_or_else(|| panic!("histogram missing for query {}", entry.query));
            let hist = decode(b64);
            assert_eq!(hist.len(), entry.count, "query {}", entry.query);
        }

        let http = report.http.expect("No http section in report");
        assert!(!http.data.is_empty());
        for entry in &http.data {
            let b64 = entry
                .histogram
                .as_ref()
                .unwrap_or_else(|| panic!("histogram missing for endpoint {}", entry.endpoint));
            let hist = decode(b64);
            assert_eq!(hist.len(), entry.count, "endpoint {}", entry.endpoint);
        }
    }

    #[test]
    fn server_sql_http_histograms_absent_without_the_cloud_feature() {
        let output = run_without_cloud("test-axum", "route_scope", None);
        let report = parse_report(&String::from_utf8_lossy(&output.stdout));

        let server = report.server.expect("No server section in report");
        assert!(server.data.iter().all(|e| e.histogram.is_none()));
        let sql = report.sql.expect("No sql section in report");
        assert!(sql.data.iter().all(|e| e.histogram.is_none()));
        let http = report.http.expect("No http section in report");
        assert!(http.data.iter().all(|e| e.histogram.is_none()));
    }

    // cargo run -p test-mutex-std --example basic_mutex_std --features hotpath,hotpath-cloud
    #[test]
    fn mutex_histograms_attached_when_upload_enabled() {
        let output = run_example("test-mutex-std", "basic_mutex_std", None, true);
        let report = parse_report(&String::from_utf8_lossy(&output.stdout));

        let mutexes = report.mutexes.expect("No mutexes section in report");
        assert!(!mutexes.data.is_empty());
        for entry in &mutexes.data {
            assert_eq!(
                entry.wait_histogram.is_some(),
                entry.sampled_count > 0,
                "wait histogram presence mismatch for {}",
                entry.source
            );
            assert_eq!(
                entry.acquire_histogram.is_some(),
                entry.sampled_count > 0,
                "acquire histogram presence mismatch for {}",
                entry.source
            );
            if let Some(b64) = &entry.wait_histogram {
                let hist = decode(b64);
                assert_eq!(hist.len(), entry.sampled_count, "source {}", entry.source);
            }
        }
    }

    #[test]
    fn mutex_histograms_absent_without_the_cloud_feature() {
        let output = run_without_cloud("test-mutex-std", "basic_mutex_std", None);
        let report = parse_report(&String::from_utf8_lossy(&output.stdout));

        let mutexes = report.mutexes.expect("No mutexes section in report");
        assert!(!mutexes.data.is_empty());
        assert!(mutexes
            .data
            .iter()
            .all(|e| e.wait_histogram.is_none() && e.acquire_histogram.is_none()));
    }
}
