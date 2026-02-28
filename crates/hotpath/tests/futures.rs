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

    // cargo run -p test-futures --example basic_futures --features hotpath
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
        let futures_path = format!("| examples{sep}basic_futures.rs:");
        let all_expected = [
            "| labeled_with_log",
            "| my_labeled_future",
            "| basic_futures::attributed_no_log   | 2     | 4",
            "| basic_futures::attributed_with_log | 2     | 4",
            futures_path.as_str(),
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-futures --example basic_futures --features hotpath
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

    // HOTPATH_METRICS_PORT=6775 TEST_SLEEP_SECONDS=10 cargo run -p test-futures --example basic_futures --features hotpath
    #[test]
    fn test_data_endpoints() {
        use hotpath::json::{DataFlowType, JsonDataFlowList};
        use std::{thread::sleep, time::Duration};

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
            .env("HOTPATH_METRICS_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        let all_expected = ["basic_futures.rs", "primary_count", "data_flow_type"];

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6775/data_flow").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    if all_expected.iter().all(|e| json_text.contains(e)) {
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
            panic!("Failed after 12 retries: {}", error);
        }

        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        let data_flow: JsonDataFlowList =
            serde_json::from_str(&json_text).expect("Failed to parse data_flow JSON");

        let first_future = data_flow
            .entries
            .iter()
            .find(|e| e.data_flow_type == DataFlowType::Future);

        if let Some(future) = first_future {
            let calls_url = format!("http://localhost:6775/data_flow/future/{}/logs", future.id);
            let mut response = ureq::get(&calls_url)
                .call()
                .expect("Failed to call /data_flow/future/{id}/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /data_flow/future/{{id}}/logs endpoint"
            );

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

    // cargo run -p test-futures --example guard_timeout_futures --features hotpath
    #[test]
    fn test_guard_timeout_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "guard_timeout_futures",
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
        let expected_content = [
            "[hotpath]",
            "| futures",
            "guard_timeout_futures::timeout_worker",
        ];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-futures --example measure_future --features hotpath
    #[test]
    fn test_measure_future_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "measure_future",
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

        let expected_timing = [
            "measure_future::timed_future ",
            "measure_future::timed_future_with_log",
            "measure_future::timing_only",
        ];

        for expected in expected_timing {
            assert!(
                stdout.contains(expected),
                "Expected in timing table:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        let expected_futures = [
            "| measure_future::timed_future          | 2     | 4",
            "| measure_future::timed_future_with_log | 1     | 2",
        ];

        for expected in expected_futures {
            assert!(
                stdout.contains(expected),
                "Expected in futures table:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        if let Some(futures_section) = stdout.split("futures - Future poll").nth(1) {
            assert!(
                !futures_section.contains("timing_only"),
                "timing_only should not appear in futures table.\nFutures section:\n{futures_section}",
            );
        }
    }

    // HOTPATH_OUTPUT_FORMAT=none cargo run -p test-futures --example basic_futures --features hotpath
    #[test]
    fn test_format_none_suppresses_output() {
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
            .env("HOTPATH_OUTPUT_FORMAT", "none")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let not_expected = [
            "[hotpath]",
            "Future poll",
            "attributed_no_log",
            "attributed_with_log",
        ];

        for not_exp in not_expected {
            assert!(
                !stdout.contains(not_exp),
                "Futures output should be suppressed with HOTPATH_OUTPUT_FORMAT=none.\nFound: {not_exp}\nGot:\n{stdout}"
            );
        }
    }

    // cargo run -p test-futures --example futures_file_output --features hotpath
    #[test]
    fn test_futures_file_output() {
        use std::fs;
        use std::path::Path;

        let output_path = "tmp/futures_output_test.json";

        fs::create_dir_all("tmp").ok();
        if Path::new(output_path).exists() {
            fs::remove_file(output_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-futures",
                "--example",
                "futures_file_output",
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

        assert!(
            Path::new(output_path).exists(),
            "Output file was not created at {}",
            output_path
        );

        let file_content = fs::read_to_string(output_path).expect("Failed to read output file");

        let expected_content = [
            "futures_file_output.rs:",
            "\"total_polls\"",
            "\"call_count\"",
        ];

        for expected in expected_content {
            assert!(
                file_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{file_content}",
            );
        }

        fs::remove_file(output_path).ok();
    }
}
