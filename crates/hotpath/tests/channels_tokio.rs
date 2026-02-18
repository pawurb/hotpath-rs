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

    // cargo run -p test-channels-tokio --example basic_tokio --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_tokio",
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

        assert!(!output.stderr.is_empty(), "Stderr is empty");
        let all_expected = [
            "Actor 1",
            "bounded-channel",
            "hello-there",
            "unbounded",
            "bounded[10]",
            "oneshot",
            "notified",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example basic_json_tokio --features hotpath
    #[test]
    fn test_basic_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_json_tokio",
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

        let sep = path_sep();
        let json_path = format!("\"label\": \"examples{sep}basic_json_tokio.rs:");
        let all_expected = [json_path.as_str(), "\"label\": \"hello-there\""];

        let stdout = String::from_utf8_lossy(&output.stdout);

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example closed_tokio --features hotpath
    #[test]
    fn test_closed_channels_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "closed_tokio",
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

        // Match "closed" with flexible spacing (table cells are padded)
        let closed_count = stdout.matches("| closed").count();
        assert_eq!(
            closed_count, 2,
            "Expected 'closed' state to appear 2 times in table (bounded and unbounded), found {}.\nOutput:\n{}",
            closed_count, stdout
        );

        let notified_count = stdout.matches("| notified").count();
        assert_eq!(
            notified_count, 1,
            "Expected 'notified' state to appear 1 time in table (oneshot), found {}.\nOutput:\n{}",
            notified_count, stdout
        );
    }

    // cargo run -p test-channels-tokio --example oneshot_closed_tokio --features hotpath
    #[test]
    fn test_oneshot_closed_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "oneshot_closed_tokio",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "Command failed with status: {}\nStdout:\n{}\nStderr:\n{}",
            output.status,
            stdout,
            stderr
        );

        let all_expected = ["| closed |", "oneshot_closed_tokio.rs:"];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example iter_tokio --features hotpath
    #[test]
    fn test_iter_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "iter_tokio",
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
        let iter_36 = format!("examples{sep}iter_tokio.rs:36");
        let iter_36_2 = format!("examples{sep}iter_tokio.rs:36-2");
        let iter_36_3 = format!("examples{sep}iter_tokio.rs:36-3");
        let iter_48 = format!("examples{sep}iter_tokio.rs:48");
        let iter_48_2 = format!("examples{sep}iter_tokio.rs:48-2");
        let iter_48_3 = format!("examples{sep}iter_tokio.rs:48-3");
        let all_expected = [
            "Actor 1",
            "Actor 1-2",
            "Actor 1-3",
            iter_36.as_str(),
            iter_36_2.as_str(),
            iter_36_3.as_str(),
            iter_48.as_str(),
            iter_48_2.as_str(),
            iter_48_3.as_str(),
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example slow_consumer_tokio --features hotpath
    #[test]
    fn test_slow_consumer_no_panic() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "slow_consumer_tokio",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "Command failed with status: {}\nStdout:\n{}\nStderr:\n{}",
            output.status,
            stdout,
            stderr
        );

        assert!(
            stdout.contains("Slow consumer example completed!"),
            "Expected completion message not found.\nOutput:\n{}",
            stdout
        );
    }

    // cargo run -p test-channels-tokio --example guard_timeout_channels --features hotpath
    #[test]
    fn test_guard_timeout_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "guard_timeout_channels",
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
        let expected_content = ["[hotpath]", "| channels", "timeout-channel"];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // HOTPATH_METRICS_PORT=6773 TEST_SLEEP_SECONDS=10 cargo run -p test-channels-tokio --example basic_tokio --features hotpath
    #[test]
    fn test_data_endpoints() {
        use hotpath::json::{DataFlowType, JsonDataFlowList};
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_tokio",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6773")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        let all_expected = ["basic_tokio.rs", "bounded-channel", "Actor 1"];

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6773/data_flow").call() {
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

        let first_channel = data_flow
            .entries
            .iter()
            .find(|e| e.data_flow_type == DataFlowType::Channel);

        if let Some(channel) = first_channel {
            let logs_url = format!(
                "http://localhost:6773/data_flow/channel/{}/logs",
                channel.id
            );
            let response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /data_flow/channel/:id/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /data_flow/channel/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }

    // HOTPATH_OUTPUT_FORMAT=none cargo run -p test-channels-tokio --example basic_tokio --features hotpath
    #[test]
    fn test_format_none_suppresses_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_tokio",
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

        assert!(
            stdout.contains("Example completed!"),
            "Application output should still be present.\nGot:\n{stdout}"
        );

        let not_expected = [
            "[hotpath]",
            "bounded-channel",
            "hello-there",
            "Channel throughput",
        ];

        for not_exp in not_expected {
            assert!(
                !stdout.contains(not_exp),
                "Channel output should be suppressed with HOTPATH_OUTPUT_FORMAT=none.\nFound: {not_exp}\nGot:\n{stdout}"
            );
        }
    }

    // cargo run -p test-channels-tokio --example channels_file_output --features hotpath
    #[test]
    fn test_channels_file_output() {
        use std::fs;
        use std::path::Path;

        let output_path = "tmp/channels_output_test.json";

        fs::create_dir_all("tmp").ok();
        if Path::new(output_path).exists() {
            fs::remove_file(output_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "channels_file_output",
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

        let expected_content = ["test-channel", "\"sent_count\"", "\"received_count\""];

        for expected in expected_content {
            assert!(
                file_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{file_content}",
            );
        }

        fs::remove_file(output_path).ok();
    }
}
