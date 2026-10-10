//! Integration tests for the ureq 3 middleware HTTP front-end.
//!
//! These run the `test-ureq` examples as subprocesses and assert on their
//! reports. The blocking ureq agent feeds the same `hp-http` worker as the
//! reqwest front-ends through a ureq `Middleware` impl, so the assertions
//! mirror `http_reqwest.rs`.
#[path = "common/support.rs"]
mod common;

#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use crate::common::assert_contains_all;
    use crate::common::example::Example;
    use crate::common::report::parse_report;

    fn example(name: &str) -> Example {
        Example::new("test-ureq", name)
    }

    #[test]
    fn test_http_table_ureq() {
        let stdout = example("basic").stdout();

        assert_contains_all(
            &stdout,
            &[
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
            ],
        );
    }

    #[test]
    fn test_http_json_ureq() {
        let http = example("basic")
            .json()
            .report()
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
        let http = example("sources")
            .json()
            .report()
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
        let stdout = example("manual_middleware").json().stdout();
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
