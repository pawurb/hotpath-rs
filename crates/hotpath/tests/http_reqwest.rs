//! Integration tests for the reqwest-middleware HTTP front-end.
//!
//! These run the `test-reqwest-012` and `test-reqwest-013` `basic` examples as
//! subprocesses and assert on their reports. Both generations feed the same
//! `hp-http` worker through generation-specific `Middleware` impls, so the
//! assertions are identical; only the wrapped reqwest version differs.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    fn run_example(package: &str, example: &str, format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            package,
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

    fn assert_table_output(package: &str) {
        let stdout = run_example(package, "basic", None);

        let all_expected = [
            "HTTP example completed",
            "http - HTTP request execution time statistics.",
            // 2 user fetches + missing + unreachable + labeled client = 5.
            "Total calls: 5",
            // Two ids and a query string merge into one normalized bucket.
            "/users/{id}",
            "/missing",
            "/unreachable",
            // Labeled client prefixes its bucket keys.
            "ext: GET",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "[{package}] Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    fn assert_json_report(package: &str) {
        use hotpath::json::JsonReport;

        let stdout = run_example(package, "basic", Some("json"));
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        let http = report.http.expect("No http section in report");

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

        let missing = http
            .data
            .iter()
            .find(|e| e.endpoint.ends_with("/missing"))
            .expect("missing bucket");
        assert_eq!(missing.count, 1);
        assert_eq!(missing.errors, 1);

        let refused = http
            .data
            .iter()
            .find(|e| e.endpoint.ends_with("/unreachable"))
            .expect("unreachable bucket");
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

    // Compiles and runs the feature-gated RequestBuilder methods (json on 0.12;
    // json/query/form on 0.13) against the wrapped client. Fails to compile if
    // the hotpath crate stops forwarding the reqwest-middleware feature flags.
    fn assert_gated_methods(package: &str) {
        let stdout = run_example(package, "gated_methods", None);
        assert!(
            stdout.contains("Gated methods example completed"),
            "[{package}] Expected completion marker, got:\n{stdout}",
        );
    }

    // The same endpoint requested from two instrumented functions splits into
    // per-source entries; a request outside any measured scope has no source,
    // and a nested measured call attributes to the innermost function.
    fn assert_sources_report(package: &str) {
        use hotpath::json::JsonReport;

        let stdout = run_example(package, "sources", Some("json"));
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        let http = report.http.expect("No http section in report");

        let users: Vec<_> = http
            .data
            .iter()
            .filter(|e| e.endpoint.ends_with("/users/{id}"))
            .collect();
        assert_eq!(users.len(), 3, "expected one entry per source: {users:?}");

        // `fetch_from_a` runs once directly and once inside `outer` - both
        // executions attribute to the innermost function.
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

    #[test]
    fn test_http_table_reqwest_012() {
        assert_table_output("test-reqwest-012");
    }

    #[test]
    fn test_http_table_reqwest_013() {
        assert_table_output("test-reqwest-013");
    }

    #[test]
    fn test_http_json_reqwest_012() {
        assert_json_report("test-reqwest-012");
    }

    #[test]
    fn test_http_json_reqwest_013() {
        assert_json_report("test-reqwest-013");
    }

    #[test]
    fn test_http_gated_methods_reqwest_012() {
        assert_gated_methods("test-reqwest-012");
    }

    #[test]
    fn test_http_gated_methods_reqwest_013() {
        assert_gated_methods("test-reqwest-013");
    }

    #[test]
    fn test_http_sources_reqwest_012() {
        assert_sources_report("test-reqwest-012");
    }

    #[test]
    fn test_http_sources_reqwest_013() {
        assert_sources_report("test-reqwest-013");
    }
}
