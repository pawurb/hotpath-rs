#[cfg(test)]
pub mod tests {
    use std::process::Command;

    fn path_sep() -> &'static str {
        if cfg!(windows) {
            "\\"
        } else {
            "/"
        }
    }

    #[test]
    fn test_basic_futures_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "basic_futures",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let sep = path_sep();
        let futures_path = format!("| examples{sep}basic_futures.rs:47");
        let all_expected = [
            "| basic_futures::attributed_no_log   | 2     | 4     |",
            "| basic_futures::attributed_with_log | 2     | 4     |",
            futures_path.as_str(),
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_futures_aggregation() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "basic_futures",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check for #[future_fn] attributed function names (aggregated)
        assert!(
            stdout.contains("attributed_no_log"),
            "Expected 'attributed_no_log' function name in output.\nOutput:\n{}",
            stdout
        );

        assert!(
            stdout.contains("attributed_with_log"),
            "Expected 'attributed_with_log' function name in output.\nOutput:\n{}",
            stdout
        );

        // Check for future locations in the output (file:line format)
        assert!(
            stdout.contains("basic_futures.rs:"),
            "Expected 'basic_futures.rs:' file location in output.\nOutput:\n{}",
            stdout
        );

        // Check that aggregation shows correct call counts and polls
        // attributed_no_log and attributed_with_log are each called 2 times
        // Each call has 2 polls, so total is 4 polls
        assert!(
            stdout.contains("| 2     | 4"),
            "Expected aggregated call count of 2 and poll count of 4.\nOutput:\n{}",
            stdout
        );
    }

    #[test]
    fn test_data_endpoints() {
        use hotpath::json::FuturesJson;
        use std::{thread::sleep, time::Duration};

        // Spawn example process
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "basic_futures",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_HTTP_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        // Test /futures endpoint
        // Give the server some time to start up
        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://127.0.0.1:6775/futures").call() {
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

        let all_expected = ["basic_futures.rs", "call_count", "total_polls"];
        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        // Test /futures/{id}/calls endpoint
        let futures_response: FuturesJson =
            serde_json::from_str(&json_text).expect("Failed to parse futures JSON");

        if let Some(first_future) = futures_response.futures.first() {
            let calls_url = format!("http://127.0.0.1:6775/futures/{}/calls", first_future.id);
            let mut response = ureq::get(&calls_url)
                .call()
                .expect("Failed to call /futures/{id}/calls endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /futures/{{id}}/calls endpoint"
            );

            // Verify the calls response contains expected data
            let calls_text = response
                .body_mut()
                .read_to_string()
                .expect("Failed to read calls response");
            assert!(
                calls_text.contains("ready") || calls_text.contains("cancelled"),
                "Expected calls response to contain state info.\nGot:\n{}",
                calls_text
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
