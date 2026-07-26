//! Integration tests for the `sqlx` tracing-layer front-end (sqlite backend).
//!
//! These run the `test-sqlx-08` and `test-sqlx-09` `basic` examples as
//! subprocesses and assert on their reports. The same hotpath layer feeds both:
//! the `sqlx::query` event field schema (`db.statement` / `summary` /
//! `elapsed_secs`) is identical across sqlx 0.8 and 0.9, so a single layer
//! covers both. These tests also pin that schema - a sqlx upgrade that renames
//! or drops those fields would empty the SQL report and fail here.
//!
//! PostgreSQL coverage lives in `sql_pg.rs`.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    fn run_basic(package: &str, format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            package,
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
            "Command failed with status: {}",
            output.status
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn assert_table_output(package: &str, completion_msg: &str) {
        let stdout = run_basic(package, None);

        let all_expected = [
            completion_msg,
            "sql - SQL query execution time statistics.",
            "INSERT INTO users (name, age) VALUES (?, ?)",
            // Short query (4 words) arrives via `summary`, not `db.statement`.
            "SELECT COUNT(*) FROM users",
            // Inline literals normalized into one bucket.
            "SELECT name FROM users WHERE age = ?",
            // Different-arity IN lists collapse to one bucket.
            "SELECT * FROM users WHERE id IN (?)",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "[{package}] Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    fn assert_transaction_queries_captured(package: &str) {
        // 50 loop inserts + 1 transaction-internal insert = 51. A pool wrapper
        // would miss the transaction-internal query; the layer captures it.
        let stdout = run_basic(package, Some("json"));

        let all_expected = [
            "\"sql\"",
            "\"INSERT INTO users (name, age) VALUES (?, ?)\"",
            "\"count\":51",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "[{package}] Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // With HOTPATH_SQL_RAW_LOGS=1, log entries keep the raw statement text -
    // inline literals included - while the bucket stays normalized.
    #[test]
    fn test_sql_raw_logs_opt_in() {
        use hotpath::json::{JsonSqlList, JsonSqlLogsList};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-sqlx-08",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6786")
            .env("HOTPATH_SQL_RAW_LOGS", "1")
            .env("TEST_SLEEP_SECONDS", "30")
            .spawn()
            .expect("Failed to spawn command");

        let literal_query = "SELECT name FROM users WHERE age = ?";
        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..40 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6786/sql").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    if json_text.contains(literal_query) {
                        break;
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!("Failed after 40 retries: {}", error);
        }

        let result = std::panic::catch_unwind(|| {
            let sql: JsonSqlList = serde_json::from_str(&json_text).expect("Failed to parse /sql");
            let entry = sql
                .data
                .iter()
                .find(|e| e.query == literal_query)
                .expect("inline-literal bucket not found in /sql");

            let logs_url = format!("http://localhost:6786/sql/{}/logs", entry.id);
            let mut response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /sql/:id/logs endpoint");
            let body = response
                .body_mut()
                .read_to_string()
                .expect("Failed to read logs body");
            let logs: JsonSqlLogsList =
                serde_json::from_str(&body).expect("Failed to parse /sql/:id/logs");

            assert!(!logs.logs.is_empty(), "Expected log entries, got: {body}");
            // The example runs `... WHERE age = 21` through `... WHERE age = 40`.
            assert!(
                logs.logs
                    .iter()
                    .any(|l| l.query == "SELECT name FROM users WHERE age = 21"),
                "Expected raw inline literal in logs, got: {body}"
            );
        });

        let _ = child.kill();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn test_table_output_sqlx_08() {
        assert_table_output("test-sqlx-08", "sqlx 0.8 tracing-layer example completed!");
    }

    #[test]
    fn test_table_output_sqlx_09() {
        assert_table_output("test-sqlx-09", "sqlx 0.9 tracing-layer example completed!");
    }

    #[test]
    fn test_transaction_queries_captured_sqlx_08() {
        assert_transaction_queries_captured("test-sqlx-08");
    }

    // Polls the live /sql endpoint, then fetches per-query execution logs from
    // /sql/{id}/logs and asserts they hold the normalized statement text.
    #[test]
    fn test_sql_logs_endpoint() {
        use hotpath::json::{JsonSqlList, JsonSqlLogsList};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-sqlx-08",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6785")
            .env("TEST_SLEEP_SECONDS", "30")
            .spawn()
            .expect("Failed to spawn command");

        let insert_query = "INSERT INTO users (name, age) VALUES (?, ?)";
        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..40 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6785/sql").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    if json_text.contains(insert_query) {
                        break;
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!("Failed after 40 retries: {}", error);
        }

        let result = std::panic::catch_unwind(|| {
            let sql: JsonSqlList = serde_json::from_str(&json_text).expect("Failed to parse /sql");
            let entry = sql
                .data
                .iter()
                .find(|e| e.query == insert_query)
                .expect("INSERT bucket not found in /sql");

            let logs_url = format!("http://localhost:6785/sql/{}/logs", entry.id);
            let mut response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /sql/:id/logs endpoint");
            assert_eq!(response.status(), 200);

            let body = response
                .body_mut()
                .read_to_string()
                .expect("Failed to read logs body");
            let logs: JsonSqlLogsList =
                serde_json::from_str(&body).expect("Failed to parse /sql/:id/logs");

            assert!(!logs.logs.is_empty(), "Expected log entries, got: {body}");
            let log = &logs.logs[0];
            assert_eq!(log.query, insert_query);
            assert!(!log.duration.is_empty());
            assert!(!log.ago.is_empty());
            // 51 inserts total, capped at HOTPATH_LOGS_LIMIT (default 50).
            assert_eq!(logs.logs.len(), 50, "Got: {body}");
            assert_eq!(log.index, 51, "newest execution first, got: {body}");

            // Log entries hold the normalized text - the example executes
            // `SELECT name FROM users WHERE age = <literal>` with varying
            // inline literals, and none of them may leak into the logs.
            let literal_query = "SELECT name FROM users WHERE age = ?";
            let entry = sql
                .data
                .iter()
                .find(|e| e.query == literal_query)
                .expect("inline-literal bucket not found in /sql");
            let logs_url = format!("http://localhost:6785/sql/{}/logs", entry.id);
            let mut response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /sql/:id/logs endpoint");
            let body = response
                .body_mut()
                .read_to_string()
                .expect("Failed to read logs body");
            let logs: JsonSqlLogsList =
                serde_json::from_str(&body).expect("Failed to parse /sql/:id/logs");
            assert!(!logs.logs.is_empty(), "Expected log entries, got: {body}");
            for log in &logs.logs {
                assert_eq!(log.query, literal_query, "literal leaked: {body}");
            }

            // Unknown id returns 404.
            let agent = ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .http_status_as_error(false)
                    .build(),
            );
            let resp = agent
                .get("http://localhost:6785/sql/999999/logs")
                .call()
                .expect("Failed to call logs endpoint with unknown id");
            assert_eq!(resp.status().as_u16(), 404, "Expected 404 for unknown id");
        });

        let _ = child.kill();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn test_transaction_queries_captured_sqlx_09() {
        assert_transaction_queries_captured("test-sqlx-09");
    }
}
