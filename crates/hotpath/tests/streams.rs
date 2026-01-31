#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-streams --example basic_streams --features hotpath
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

    // cargo run -p test-streams --example basic_streams --features hotpath
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

    // HOTPATH_METRICS_PORT=6774 TEST_SLEEP_SECONDS=10 cargo run -p test-streams --example basic_streams --features hotpath
    #[test]
    fn test_data_endpoints() {
        use hotpath::json::{DataFlowType, JsonDataFlowList};
        use std::{thread::sleep, time::Duration};

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
            .env("HOTPATH_METRICS_PORT", "6774")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://localhost:6774/data_flow").call() {
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

        let all_expected = ["basic_streams.rs", "number-stream", "text-stream"];
        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        let data_flow: JsonDataFlowList =
            serde_json::from_str(&json_text).expect("Failed to parse data_flow JSON");

        let first_stream = data_flow
            .entries
            .iter()
            .find(|e| e.data_flow_type == DataFlowType::Stream);

        if let Some(stream) = first_stream {
            let logs_url = format!("http://localhost:6774/data_flow/stream/{}/logs", stream.id);
            let response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /data_flow/stream/:id/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /data_flow/stream/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }

    // cargo run -p test-streams --example streams_file_output --features hotpath
    #[test]
    fn test_streams_file_output() {
        use std::fs;
        use std::path::Path;

        let output_path = "tmp/streams_output_test.json";

        fs::create_dir_all("tmp").ok();
        if Path::new(output_path).exists() {
            fs::remove_file(output_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-streams",
                "--example",
                "streams_file_output",
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

        let expected_content = ["number-stream", "\"items_yielded\""];

        for expected in expected_content {
            assert!(
                file_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{file_content}",
            );
        }

        fs::remove_file(output_path).ok();
    }
}
