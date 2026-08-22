//! Integration tests for route scoping: SQL queries and outbound HTTP requests
//! issued inside an axum handler carry the matched route template.
//!
//! These run the `test-axum` `route_scope` example as a subprocess and assert
//! on the JSON report printed when the guard drops.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::JsonReport;
    use std::process::Command;

    fn run_example(route_scope: Option<&str>) -> JsonReport {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-axum",
            "--example",
            "route_scope",
            "--features",
            "hotpath",
        ])
        .env("HOTPATH_OUTPUT_FORMAT", "json");
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
        let stdout = String::from_utf8_lossy(&output.stdout);
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
    }
}
