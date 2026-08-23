//! Integration tests for route scoping: SQL queries and outbound HTTP requests
//! issued inside an axum handler carry the matched route template.
//!
//! These run the `test-axum` `route_scope` example as a subprocess and assert
//! on the JSON report printed when the guard drops.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::JsonReport;
    use std::process::Command;

    fn run_example_raw(route_scope: Option<&str>, format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-axum",
            "--example",
            "route_scope",
            "--features",
            "hotpath",
        ]);
        if let Some(format) = format {
            cmd.env("HOTPATH_OUTPUT_FORMAT", format);
        }
        if let Some(value) = route_scope {
            cmd.env("HOTPATH_ROUTE_SCOPE", value);
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

    fn run_example(route_scope: Option<&str>) -> JsonReport {
        let stdout = run_example_raw(route_scope, Some("json"));
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
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

        // The server section derives per-request SQL / HTTP averages from
        // the same attribution.
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
        assert_eq!(users.http_per_request, None);
        // Unmatched requests carry no route scope.
        let missing = by_route("GET /missing");
        assert_eq!(missing.count, 1);
        assert_eq!(missing.sql_per_request, None);
        assert_eq!(missing.http_per_request, None);
    }

    #[test]
    fn test_route_scope_server_table_columns() {
        let stdout = run_example_raw(None, None);
        for expected in ["SQL/req", "HTTP/req", "GET /profiles/{id}"] {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
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
            .find(|l| l.contains("GET /missing"))
            .expect("missing row missing");
        let cells: Vec<&str> = missing.split('|').map(str::trim).collect();
        assert_eq!(
            &cells[1..7],
            &["GET /missing", "1", "1", "0", "-", "-"],
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
