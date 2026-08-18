//! Integration tests for the axum 0.8 server-side response time front-end.
//!
//! These run the `test-axum` `basic` example as a subprocess and assert on its
//! report and on the live `/server` metrics endpoint. The tower layer feeds
//! the `hp-server` worker, which buckets requests by matched route template.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::{JsonHttpLogsList, JsonReport, JsonServerList};
    use std::process::Command;

    fn run_example(format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-axum",
            "--example",
            "basic",
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
    fn test_server_table_axum() {
        let stdout = run_example(None);

        let all_expected = [
            "axum example completed",
            "server - HTTP server response time statistics per route.",
            // 3 user fetches + create + crash + unmatched = 6.
            "Total requests: 6",
            // Matched routes report the router's own template.
            "GET /users/{id}",
            "POST /users",
            "GET /crash",
            // Unmatched requests fall back to the normalized raw path.
            "GET /missing/{id}",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_server_json_axum() {
        let stdout = run_example(Some("json"));
        let server = parse_report(&stdout)
            .server
            .expect("No server section in report");

        assert_eq!(server.total_calls, 6);
        assert_eq!(
            server.total_calls,
            server.data.iter().map(|e| e.count).sum::<u64>()
        );

        let find = |route: &str| {
            server
                .data
                .iter()
                .find(|e| e.route == route)
                .unwrap_or_else(|| panic!("{route} bucket missing"))
        };

        // Two ids and a query string merge into the route template; the
        // matched-but-404 lookup lands in the same bucket as a 4xx.
        let users = find("GET /users/{id}");
        assert_eq!(users.count, 3);
        assert_eq!(users.status_4xx, 1);
        assert_eq!(users.status_5xx, 0);

        let created = find("POST /users");
        assert_eq!(created.count, 1);
        assert_eq!(created.status_4xx, 0);
        assert_eq!(created.status_5xx, 0);

        let crash = find("GET /crash");
        assert_eq!(crash.count, 1);
        assert_eq!(crash.status_5xx, 1);

        let unmatched = find("GET /missing/{id}");
        assert_eq!(unmatched.count, 1);
        assert_eq!(unmatched.status_4xx, 1);
    }

    // HOTPATH_METRICS_PORT=6787 TEST_SLEEP_SECONDS=10 cargo run -p test-axum --example basic --features hotpath
    #[test]
    fn test_server_endpoints_axum() {
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-axum",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6787")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut server: Option<JsonServerList> = None;
        let mut last_error = None;

        for _attempt in 0..40 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6787/server").call() {
                Ok(mut response) => {
                    let body = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    let parsed: JsonServerList =
                        serde_json::from_str(&body).expect("Failed to parse /server");
                    last_error = None;
                    if parsed.total_calls == 6 {
                        server = Some(parsed);
                        break;
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        let Some(server) = server else {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Never observed 6 served requests: {last_error:?}");
        };

        let users = server
            .data
            .iter()
            .find(|e| e.route == "GET /users/{id}")
            .expect("users bucket missing");
        assert_eq!(users.count, 3);

        let logs_url = format!("http://localhost:6787/server/{}/logs", users.id);
        let mut response = ureq::get(&logs_url).call().expect("GET server logs");
        let body = response
            .body_mut()
            .read_to_string()
            .expect("Failed to read logs body");
        let logs: JsonHttpLogsList = serde_json::from_str(&body).expect("Failed to parse logs");
        assert_eq!(logs.logs.len(), 3);
        // Newest first: the last GET /users/{id} was the 404 lookup.
        assert_eq!(logs.logs[0].status, "404");
        assert!(logs.logs.iter().skip(1).all(|l| l.status == "200"));

        let missing = ureq::get("http://localhost:6787/server/999999/logs").call();
        assert!(matches!(missing, Err(ureq::Error::StatusCode(404))));

        let _ = child.kill();
        let _ = child.wait();
    }
}
