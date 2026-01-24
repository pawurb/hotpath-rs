#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // HOTPATH_METRICS_PORT=6780 TEST_SLEEP_MS=5000 cargo run -p test-debug --example basic_debug --features hotpath
    #[test]
    fn test_debug_endpoints() {
        use hotpath::json::{FormattedDbgJson, FormattedDbgLogs};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-debug",
                "--example",
                "basic_debug",
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
            sleep(Duration::from_millis(500));

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

        let debug_response: FormattedDbgJson =
            serde_json::from_str(&json_text).expect("Failed to parse debug JSON");

        let first = debug_response.debug_logs.first().expect("No debug logs");

        assert!(
            !first.source.is_empty() && !first.expression.is_empty() && first.log_count >= 1,
            "Debug response missing expected fields"
        );

        use base64::Engine;
        let encoded_source =
            base64::engine::general_purpose::STANDARD.encode(first.source.as_bytes());
        let logs_json = ureq::get(&format!(
            "http://localhost:6780/debug/{}/logs",
            encoded_source
        ))
        .call()
        .expect("Failed to call /debug/:source/logs endpoint")
        .body_mut()
        .read_to_string()
        .expect("Failed to read logs response body");

        let logs: FormattedDbgLogs =
            serde_json::from_str(&logs_json).expect("Failed to parse debug logs JSON");

        let first_log = logs.logs.first().expect("No log entries");
        assert!(
            !logs.source.is_empty() && logs.total_logs >= 1 && !first_log.value.is_empty(),
            "Logs response missing expected fields"
        );

        let _ = child.kill();
        let _ = child.wait();
    }
}
