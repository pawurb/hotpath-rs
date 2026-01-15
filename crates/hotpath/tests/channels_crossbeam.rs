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

    // cargo run -p test-channels-crossbeam --example basic_crossbeam --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "basic_crossbeam",
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
        let sep = path_sep();
        let basic_crossbeam_path = format!("examples{sep}basic_crossbeam.rs");
        let all_expected = [
            basic_crossbeam_path.as_str(),
            "hello-there",
            "unbounded",
            "bounded[10]",
            "bounded[1]",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-crossbeam --example closed_crossbeam --features hotpath
    #[test]
    fn test_closed_channels_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "closed_crossbeam",
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

        // Check that all three channels have "closed" state
        assert!(
            stdout.contains("closed-sender"),
            "Expected closed-sender channel in output"
        );
        assert!(
            stdout.contains("closed-receiver"),
            "Expected closed-receiver channel in output"
        );
        assert!(
            stdout.contains("closed-unbounded"),
            "Expected closed-unbounded channel in output"
        );
    }

    // cargo run -p test-channels-crossbeam --example basic_json_crossbeam --features hotpath
    #[test]
    fn test_basic_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "basic_json_crossbeam",
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

        let all_expected = ["\"label\": \"bounded\"", "\"label\": \"unbounded\""];

        let stdout = String::from_utf8_lossy(&output.stdout);

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-crossbeam --example iter_crossbeam --features hotpath
    #[test]
    fn test_iter_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "iter_crossbeam",
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
        let iter_path = format!("examples{sep}iter_crossbeam.rs:12");
        let iter_path_2 = format!("examples{sep}iter_crossbeam.rs:12-2");
        let iter_path_3 = format!("examples{sep}iter_crossbeam.rs:12-3");
        let all_expected = [
            "bounded",
            "bounded-2",
            "bounded-3",
            iter_path.as_str(),
            iter_path_2.as_str(),
            iter_path_3.as_str(),
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-crossbeam --example slow_consumer_crossbeam --features hotpath
    #[test]
    fn test_slow_consumer_no_panic() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "slow_consumer_crossbeam",
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

    // HOTPATH_METRICS_PORT=6771 TEST_SLEEP_SECONDS=10 cargo run -p test-channels-crossbeam --example basic_crossbeam --features hotpath
    #[test]
    fn test_data_endpoints() {
        use hotpath::json::ChannelsJson;
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "basic_crossbeam",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6771")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        // Test /channels endpoint
        // Give the server some time to start up

        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://localhost:6771/channels").call() {
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

        let all_expected = ["basic_crossbeam.rs", "unbounded", "hello-there"];
        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        // Test /channels/:id/logs endpoint
        let channels_response: ChannelsJson =
            serde_json::from_str(&json_text).expect("Failed to parse channels JSON");

        if let Some(first_channel) = channels_response.channels.first() {
            let logs_url = format!("http://localhost:6771/channels/{}/logs", first_channel.id);
            let response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /channels/:id/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /channels/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
