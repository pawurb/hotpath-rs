#[cfg(test)]
pub mod tests {
    use std::process::Command;

    #[test]
    fn test_basic_streams_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-streams",
                "--example",
                "basic_streams",
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

        let all_expected = [
            "number-stream",
            "text-stream",
            "repeat-stream",
            "Stream example completed!",
            "Streams:",
            "5", // number-stream yielded 5 items
            "4", // text-stream yielded 4 items
            "3", // repeat-stream yielded 3 items
            "Yielded",
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_streams_closed_state() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-streams",
                "--example",
                "basic_streams",
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

        // All streams should be in closed state after completion
        let closed_count = stdout.matches("| closed").count();
        assert!(
            closed_count >= 3,
            "Expected at least 3 'closed' states for streams, found {}.\nOutput:\n{}",
            closed_count,
            stdout
        );
    }

    #[test]
    fn test_data_endpoints() {
        use hotpath::streams::StreamsJson;
        use std::{thread::sleep, time::Duration};

        // Spawn example process
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-streams",
                "--example",
                "basic_streams",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_HTTP_PORT", "6774")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        // Test /streams endpoint
        // Give the server some time to start up

        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://127.0.0.1:6774/streams").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    break;
                }
                Err(e) => {
                    dbg!(&e);
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!("Failed after 12 retries: {}", error);
        }

        let all_expected = ["basic_streams.rs", "number-stream", "text-stream"];
        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        // Test /streams/:id/logs endpoint
        let streams_response: StreamsJson =
            serde_json::from_str(&json_text).expect("Failed to parse streams JSON");

        if let Some(first_stream) = streams_response.streams.first() {
            let logs_url = format!("http://127.0.0.1:6774/streams/{}/logs", first_stream.id);
            let response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /streams/:id/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /streams/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
