#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    // HOTPATH_METRICS_PORT=6780 TEST_SLEEP_MS=5000 cargo run -p test-debug --example basic_dbg --features hotpath
    #[test]
    fn test_dbg_endpoints() {
        use hotpath::json::{DebugEntryType, JsonDebugDbgLogs, JsonDebugList};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-debug",
                "--example",
                "basic_dbg",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6780")
            .env("TEST_SLEEP_MS", "5000")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6780/debug").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    break;
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!("Failed after 12 retries: {}", error);
        }

        let debug_response: JsonDebugList =
            serde_json::from_str(&json_text).expect("Failed to parse debug JSON");

        let first = debug_response.entries.first().expect("No debug logs");

        assert!(
            matches!(first.entry_type, DebugEntryType::Dbg),
            "Expected entry_type to be Dbg"
        );
        assert!(
            !first.source.is_empty() && !first.expression.is_empty() && first.log_count >= 1,
            "Debug response missing expected fields"
        );

        let logs_json = ureq::get(&format!(
            "http://localhost:6780/debug/dbg/{}/logs",
            first.id
        ))
        .call()
        .expect("Failed to call /debug/dbg/:id/logs endpoint")
        .body_mut()
        .read_to_string()
        .expect("Failed to read logs response body");

        let logs: JsonDebugDbgLogs =
            serde_json::from_str(&logs_json).expect("Failed to parse debug logs JSON");

        let first_log = logs.logs.first().expect("No log entries");
        assert!(
            !logs.source.is_empty() && logs.total_logs >= 1 && !first_log.value.is_empty(),
            "Logs response missing expected fields"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    // HOTPATH_METRICS_PORT=6781 TEST_SLEEP_MS=5000 cargo run -p test-debug --example basic_val --features hotpath
    #[test]
    fn test_val_endpoints() {
        use hotpath::json::{DebugEntryType, JsonDebugList, JsonDebugValLogs};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-debug",
                "--example",
                "basic_val",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6781")
            .env("TEST_SLEEP_MS", "5000")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6781/debug").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    break;
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!("Failed after 12 retries: {}", error);
        }

        let debug_response: JsonDebugList =
            serde_json::from_str(&json_text).expect("Failed to parse debug JSON");

        assert!(
            !debug_response.entries.is_empty(),
            "Expected at least one debug log entry"
        );

        for entry in &debug_response.entries {
            assert!(
                matches!(entry.entry_type, DebugEntryType::Val),
                "Expected entry_type to be Val, got {:?}",
                entry.entry_type
            );
        }

        let expressions: Vec<&str> = debug_response
            .entries
            .iter()
            .map(|e| e.expression.as_str())
            .collect();
        assert!(
            expressions.contains(&"counter"),
            "Expected 'counter' in expressions"
        );
        assert!(
            expressions.contains(&"status_1"),
            "Expected 'status_1' in expressions"
        );

        let counter_entry = debug_response
            .entries
            .iter()
            .find(|e| e.expression == "counter")
            .expect("counter entry not found");
        assert!(
            counter_entry.log_count >= 2,
            "Expected counter to have at least 2 logs"
        );

        let logs_json = ureq::get(&format!(
            "http://localhost:6781/debug/val/{}/logs",
            counter_entry.id
        ))
        .call()
        .expect("Failed to call /debug/val/:id/logs endpoint")
        .body_mut()
        .read_to_string()
        .expect("Failed to read logs response body");

        let logs: JsonDebugValLogs =
            serde_json::from_str(&logs_json).expect("Failed to parse val logs JSON");

        assert_eq!(logs.key, "counter", "Expected key to be 'counter'");
        assert!(logs.total_logs >= 2, "Expected at least 2 logs for counter");
        assert!(!logs.logs.is_empty(), "Expected log entries");

        let _ = child.kill();
        let _ = child.wait();
    }

    // HOTPATH_METRICS_PORT=6782 TEST_SLEEP_MS=5000 cargo run -p test-debug --example basic_gauge --features hotpath
    #[test]
    fn test_gauge_endpoints() {
        use hotpath::json::{DebugEntryType, JsonDebugGaugeLogs, JsonDebugList};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-debug",
                "--example",
                "basic_gauge",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6782")
            .env("TEST_SLEEP_MS", "5000")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6782/debug").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    break;
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!("Failed after 12 retries: {}", error);
        }

        let debug_response: JsonDebugList =
            serde_json::from_str(&json_text).expect("Failed to parse debug JSON");

        let gauge_entries: Vec<_> = debug_response
            .entries
            .iter()
            .filter(|e| matches!(e.entry_type, DebugEntryType::Gauge))
            .collect();

        assert!(
            !gauge_entries.is_empty(),
            "Expected at least one gauge entry"
        );

        let labels: Vec<&str> = gauge_entries
            .iter()
            .map(|e| e.expression.as_str())
            .collect();
        assert!(
            labels.contains(&"queue_size"),
            "Expected 'queue_size' in gauge labels"
        );
        assert!(
            labels.contains(&"connections_1"),
            "Expected 'connections_1' in gauge labels"
        );

        let queue_size_entry = gauge_entries
            .iter()
            .find(|e| e.expression == "queue_size")
            .expect("queue_size entry not found");

        assert!(
            queue_size_entry.log_count >= 3,
            "Expected queue_size to have at least 3 updates (set, inc, dec)"
        );

        let logs_json = ureq::get(&format!(
            "http://localhost:6782/debug/gauge/{}/logs",
            queue_size_entry.id
        ))
        .call()
        .expect("Failed to call /debug/gauge/:id/logs endpoint")
        .body_mut()
        .read_to_string()
        .expect("Failed to read logs response body");

        let logs: JsonDebugGaugeLogs =
            serde_json::from_str(&logs_json).expect("Failed to parse gauge logs JSON");

        assert_eq!(logs.key, "queue_size", "Expected key to be 'queue_size'");
        assert!(
            logs.total_logs >= 3,
            "Expected at least 3 logs for queue_size"
        );
        assert!(!logs.logs.is_empty(), "Expected log entries");

        let _ = child.kill();
        let _ = child.wait();
    }

    // HOTPATH_OUTPUT_FORMAT=json HOTPATH_REPORT=debug HOTPATH_METRICS_SERVER_OFF=true cargo run -p test-debug --example debug_report --features hotpath
    #[test]
    fn test_debug_report_json() {
        let output = Command::new("cargo")
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .env("HOTPATH_REPORT", "debug")
            .env("HOTPATH_METRICS_SERVER_OFF", "true")
            .args([
                "run",
                "-p",
                "test-debug",
                "--example",
                "debug_report",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let report: serde_json::Value =
            serde_json::from_str(stdout.lines().last().expect("No JSON output line"))
                .expect("Failed to parse JSON report");

        let entries = report["debug"]["entries"]
            .as_array()
            .expect("Expected debug.entries array");

        assert!(
            !entries.is_empty(),
            "Expected at least one debug entry in report"
        );

        // Check gauge entries
        let gauge_entries: Vec<_> = entries
            .iter()
            .filter(|e| e["entry_type"].as_str() == Some("gauge"))
            .collect();
        let gauge_keys: Vec<&str> = gauge_entries
            .iter()
            .filter_map(|e| e["expression"].as_str())
            .collect();
        assert!(
            gauge_keys.contains(&"queue_size"),
            "Expected 'queue_size' gauge in report, got: {:?}",
            gauge_keys
        );
        assert!(
            gauge_keys.contains(&"connections"),
            "Expected 'connections' gauge in report, got: {:?}",
            gauge_keys
        );

        // Check queue_size has the right update count (set + inc + dec = 3)
        let queue_size = gauge_entries
            .iter()
            .find(|e| e["expression"].as_str() == Some("queue_size"))
            .expect("queue_size entry not found");
        assert!(
            queue_size["log_count"].as_u64().unwrap_or(0) >= 3,
            "Expected queue_size to have at least 3 updates"
        );
        assert_eq!(
            queue_size["last_value"].as_str(),
            Some("12"),
            "Expected queue_size value to be 12 (set 10, inc 5, dec 3)"
        );

        // Check val entries
        let val_keys: Vec<&str> = entries
            .iter()
            .filter(|e| e["entry_type"].as_str() == Some("val"))
            .filter_map(|e| e["expression"].as_str())
            .collect();
        assert!(
            val_keys.contains(&"counter"),
            "Expected 'counter' val in report, got: {:?}",
            val_keys
        );
        assert!(
            val_keys.contains(&"status"),
            "Expected 'status' val in report, got: {:?}",
            val_keys
        );

        // Check dbg entries
        let dbg_entries: Vec<_> = entries
            .iter()
            .filter(|e| e["entry_type"].as_str() == Some("dbg"))
            .collect();
        assert!(
            !dbg_entries.is_empty(),
            "Expected at least one dbg entry in report"
        );

        // All entries should have non-empty source and log_count >= 1
        for entry in entries {
            assert!(
                !entry["source"].as_str().unwrap_or("").is_empty(),
                "Expected non-empty source for entry: {:?}",
                entry
            );
            assert!(
                entry["log_count"].as_u64().unwrap_or(0) >= 1,
                "Expected log_count >= 1 for entry: {:?}",
                entry
            );
        }
    }

    // HOTPATH_REPORT=debug HOTPATH_METRICS_SERVER_OFF=true cargo run -p test-debug --example debug_report --features hotpath
    #[test]
    fn test_debug_report_table() {
        let output = Command::new("cargo")
            .env("HOTPATH_REPORT", "debug")
            .env("HOTPATH_METRICS_SERVER_OFF", "true")
            .args([
                "run",
                "-p",
                "test-debug",
                "--example",
                "debug_report",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let expected_content = ["debug", "gauge!", "val!", "dbg!", "queue_size", "counter"];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected '{}' in table output.\n\nGot:\n{}",
                expected,
                stdout
            );
        }
    }
}
