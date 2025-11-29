#[cfg(test)]
pub mod tests {
    use std::process::Command;

    #[test]
    fn test_basic_tasks_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tasks",
                "--example",
                "basic_tasks",
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
            "=== Future Instrumentation Demo ===",
            "Without log = true",
            "With log = true",
            "Future was dropped without being awaited",
            "=== Demo Complete ===",
            "=== Future Statistics",
            "Futures:",
            "Future",
            "State",
            "Polls",
            "Result",
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_futures_states() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tasks",
                "--example",
                "basic_tasks",
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

        // Check for ready states (completed futures)
        let ready_count = stdout.matches("| ready").count();
        assert!(
            ready_count >= 6,
            "Expected at least 6 'ready' states for futures, found {}.\nOutput:\n{}",
            ready_count,
            stdout
        );

        // Check for cancelled state (the dropped future)
        assert!(
            stdout.contains("| cancelled"),
            "Expected 'cancelled' state for dropped future.\nOutput:\n{}",
            stdout
        );
    }

    #[test]
    fn test_futures_results() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tasks",
                "--example",
                "basic_tasks",
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

        // Check for N/A results (futures without log=true)
        let na_count = stdout.matches("| N/A").count();
        assert!(
            na_count >= 2,
            "Expected at least 2 'N/A' results for futures without log=true, found {}.\nOutput:\n{}",
            na_count,
            stdout
        );

        // Check for actual logged results
        assert!(
            stdout.contains("| 42"),
            "Expected '42' result for slow_operation with log=true.\nOutput:\n{}",
            stdout
        );

        assert!(
            stdout.contains("\"Hello World\""),
            "Expected 'Hello World' result for multi_step_operation.\nOutput:\n{}",
            stdout
        );

        // Check for dash result (cancelled future)
        assert!(
            stdout.contains("| -"),
            "Expected '-' result for cancelled future.\nOutput:\n{}",
            stdout
        );
    }

    #[test]
    fn test_data_endpoints() {
        use hotpath::tasks::TasksJson;
        use std::{thread::sleep, time::Duration};

        // Spawn example process
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tasks",
                "--example",
                "basic_tasks",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_HTTP_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        // Test /tasks endpoint
        // Give the server some time to start up
        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://127.0.0.1:6775/tasks").call() {
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

        let all_expected = ["basic_tasks.rs", "ready", "cancelled"];
        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        // Test /tasks/:id/logs endpoint
        let tasks_response: TasksJson =
            serde_json::from_str(&json_text).expect("Failed to parse tasks JSON");

        if let Some(first_task) = tasks_response.tasks.first() {
            let logs_url = format!("http://127.0.0.1:6775/tasks/{}/logs", first_task.id);
            let response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /tasks/:id/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /tasks/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
