//! Integration tests for the ureq 3 middleware HTTP front-end.
//!
//! These run the `test-ureq` examples as subprocesses and assert on their
//! reports. The blocking ureq agent feeds the same `hp-http` worker as the
//! reqwest front-ends through a ureq `Middleware` impl, so the assertions
//! mirror `http_reqwest.rs`.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::JsonReport;
    use std::process::Command;

    fn run_example(example: &str, format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-ureq",
            "--example",
            example,
            "--features",
            "hotpath",
        ]);
        if let Some(fmt) = format {
            cmd.env("HOTPATH_OUTPUT_FORMAT", fmt);
        }
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Command failed with status: {}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn parse_report(stdout: &str) -> JsonReport {
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
    }

    #[test]
    fn test_http_table_ureq() {
        let stdout = run_example("basic", None);

        let all_expected = [
            "HTTP example completed",
            "http - HTTP request execution time statistics.",
            // 2 user fetches + 404 + connection refused + labeled agent = 5.
            "Total calls: 5",
            // Two ids and a query string merge into one normalized bucket.
            "/users/{id}",
            "/stats",
            "/health",
            // Labeled agent prefixes its bucket keys.
            "ext: GET",
            // Sync instrumented methods attribute their requests.
            "Data::fetch_user",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_http_json_ureq() {
        let stdout = run_example("basic", Some("json"));
        let http = parse_report(&stdout)
            .http
            .expect("No http section in report");

        assert_eq!(http.total_calls, 5);
        assert_eq!(
            http.total_calls,
            http.data.iter().map(|e| e.count).sum::<u64>()
        );

        let users = http
            .data
            .iter()
            .find(|e| e.endpoint.starts_with("GET ") && e.endpoint.ends_with("/users/{id}"))
            .expect("users bucket missing");
        assert_eq!(users.count, 2);
        assert_eq!(users.errors, 0);
        assert_eq!(users.source.as_deref(), Some("basic::Data::fetch_user"));

        // ureq reports the 404 as `Error::StatusCode`; the middleware recovers
        // the status so it counts as an error rather than a transport failure.
        let not_found = http
            .data
            .iter()
            .find(|e| e.endpoint.ends_with("/stats"))
            .expect("404 bucket");
        assert_eq!(not_found.count, 1);
        assert_eq!(not_found.errors, 1);

        let refused = http
            .data
            .iter()
            .find(|e| e.endpoint.ends_with("/health"))
            .expect("connection-refused bucket");
        assert_eq!(refused.count, 1);
        assert_eq!(refused.errors, 1);

        let labeled = http
            .data
            .iter()
            .find(|e| e.endpoint.starts_with("ext: GET "))
            .expect("labeled bucket");
        assert_eq!(labeled.count, 1);
        assert!(labeled.endpoint.ends_with("/users/{id}"));
    }

    // The same endpoint requested from two instrumented functions splits into
    // per-source entries; a request outside any measured scope has no source,
    // and a nested measured call attributes to the innermost function.
    #[test]
    fn test_http_sources_ureq() {
        let stdout = run_example("sources", Some("json"));
        let http = parse_report(&stdout)
            .http
            .expect("No http section in report");

        let users: Vec<_> = http
            .data
            .iter()
            .filter(|e| e.endpoint.ends_with("/users/{id}"))
            .collect();
        assert_eq!(users.len(), 3, "expected one entry per source: {users:?}");

        let from_a = users
            .iter()
            .find(|e| e.source.as_deref() == Some("sources::fetch_from_a"))
            .expect("fetch_from_a entry missing");
        assert_eq!(from_a.count, 2);

        let from_b = users
            .iter()
            .find(|e| e.source.as_deref() == Some("sources::fetch_from_b"))
            .expect("fetch_from_b entry missing");
        assert_eq!(from_b.count, 2);

        let unattributed = users
            .iter()
            .find(|e| e.source.is_none())
            .expect("source-less entry missing");
        assert_eq!(unattributed.count, 1);
    }

    // Manually attached middleware (no `http!` macro) reports under its label
    // and the enclosing instrumented function.
    #[test]
    fn test_http_manual_middleware_ureq() {
        let stdout = run_example("manual_middleware", Some("json"));
        assert!(stdout.contains("HTTP manual middleware example completed"));
        let http = parse_report(&stdout)
            .http
            .expect("No http section in report");

        assert_eq!(http.total_calls, 3);
        let entry = http
            .data
            .iter()
            .find(|e| e.endpoint.starts_with("manual: GET "))
            .expect("labeled manual bucket");
        assert_eq!(entry.count, 3);
        assert_eq!(
            entry.source.as_deref(),
            Some("manual_middleware::fetch_users")
        );
    }
}
