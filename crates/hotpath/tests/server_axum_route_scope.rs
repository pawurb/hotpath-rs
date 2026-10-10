//! Integration tests for route scoping: SQL queries and outbound HTTP requests
//! issued inside an axum handler carry the matched route template.
//!
//! These run the `test-axum` `route_scope` example as a subprocess and assert
//! on the JSON report printed when the guard drops.
#[path = "common/support.rs"]
mod common;

#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::JsonReport;

    use crate::common::assert_contains_all;
    use crate::common::example::Example;

    fn example(route_scope: Option<&str>) -> Example {
        let example = Example::new("test-axum", "route_scope");
        match route_scope {
            Some(value) => example.env("HOTPATH_ROUTE_SCOPE", value),
            None => example,
        }
    }

    fn run_example(route_scope: Option<&str>) -> JsonReport {
        example(route_scope).json().report()
    }

    const LOOKUP: &str = "SELECT id, name FROM users WHERE id = ?";

    #[test]
    fn test_route_scope_attributes_sql_and_http() {
        let report = run_example(None);
        let sql = report.sql.expect("No sql section in report");
        let http = report.http.expect("No http section in report");

        // The same statement from the same source splits per route.
        let lookups: Vec<_> = sql.data.iter().filter(|e| e.query == LOOKUP).collect();
        assert_eq!(
            lookups.len(),
            2,
            "expected one entry per route: {lookups:?}"
        );
        let by_route = |route: &str| {
            lookups
                .iter()
                .find(|e| e.route.as_deref() == Some(route))
                .unwrap_or_else(|| panic!("{route} entry missing: {lookups:?}"))
        };
        // 2 direct + 3 issued from /profiles/{id} handlers.
        assert_eq!(by_route("GET /users/{id}").count, 5);
        assert_eq!(by_route("GET /profiles/{id}").count, 3);
        assert!(lookups
            .iter()
            .all(|e| e.source.as_deref() == Some("route_scope::load_user")));

        // Queries outside any handler carry no route.
        let seed = sql
            .data
            .iter()
            .find(|e| e.query.starts_with("INSERT INTO users"))
            .expect("seed insert missing");
        assert_eq!(seed.route, None);

        assert_eq!(http.data.len(), 1);
        let outbound = &http.data[0];
        assert!(outbound.endpoint.ends_with("/users/{id}"), "{outbound:?}");
        assert_eq!(outbound.count, 3);
        assert_eq!(outbound.route.as_deref(), Some("GET /profiles/{id}"));

        // Per-request SQL / HTTP averages from each request's counts.
        let server = report.server.expect("No server section in report");
        let by_route = |route: &str| {
            server
                .data
                .iter()
                .find(|e| e.route == route)
                .unwrap_or_else(|| panic!("{route} server entry missing: {:?}", server.data))
        };
        // 3 requests, each: load_user + count_users + 1 outbound request.
        let profiles = by_route("GET /profiles/{id}");
        assert_eq!(profiles.count, 3);
        assert_eq!(profiles.sql_per_request, Some(2.0));
        assert_eq!(profiles.http_per_request, Some(1.0));
        // 5 requests, one query each, no outbound request.
        let users = by_route("GET /users/{id}");
        assert_eq!(users.count, 5);
        assert_eq!(users.sql_per_request, Some(1.0));
        assert_eq!(users.http_per_request, Some(0.0));
        // Unmatched requests carry no route scope; the 404 lands in the
        // per-method unmatched bucket.
        let missing = by_route("GET <unmatched>");
        assert_eq!(missing.count, 1);
        assert_eq!(missing.sql_per_request, None);
        assert_eq!(missing.http_per_request, None);
    }

    #[test]
    fn test_route_scope_server_table_columns() {
        let stdout = example(None).stdout();
        assert_contains_all(&stdout, &["SQL/req", "HTTP/req", "GET /profiles/{id}"]);
        let profiles = stdout
            .lines()
            .find(|l| l.contains("GET /profiles/{id}"))
            .expect("profiles row missing");
        let cells: Vec<&str> = profiles.split('|').map(str::trim).collect();
        // Route | Calls | 4xx | 5xx | SQL/req | HTTP/req | ...
        assert_eq!(
            &cells[1..7],
            &["GET /profiles/{id}", "3", "0", "0", "2.0", "1.0"],
            "{profiles}"
        );
        let missing = stdout
            .lines()
            .find(|l| l.contains("GET <unmatched>"))
            .expect("unmatched row missing");
        let cells: Vec<&str> = missing.split('|').map(str::trim).collect();
        assert_eq!(
            &cells[1..7],
            &["GET <unmatched>", "1", "1", "0", "-", "-"],
            "{missing}"
        );
    }

    #[test]
    fn test_route_scope_disabled_collapses_entries() {
        let report = run_example(Some("0"));
        let sql = report.sql.expect("No sql section in report");
        let http = report.http.expect("No http section in report");

        assert!(sql.data.iter().all(|e| e.route.is_none()), "{:?}", sql.data);
        let lookups: Vec<_> = sql.data.iter().filter(|e| e.query == LOOKUP).collect();
        assert_eq!(lookups.len(), 1, "{lookups:?}");
        assert_eq!(lookups[0].count, 8);
        assert_eq!(lookups[0].source.as_deref(), Some("route_scope::load_user"));

        assert_eq!(http.data.len(), 1);
        assert_eq!(http.data[0].route, None);

        // Without route attribution nothing can be derived per route.
        let server = report.server.expect("No server section in report");
        assert!(
            server
                .data
                .iter()
                .all(|e| e.sql_per_request.is_none() && e.http_per_request.is_none()),
            "{:?}",
            server.data
        );
    }
}
