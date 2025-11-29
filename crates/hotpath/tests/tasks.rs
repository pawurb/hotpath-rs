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
            "Using #[future_fn] attribute macro",
            "=== Demo Complete ===",
            "=== Future Statistics",
            "Futures:",
            "Future",
            "Calls",
            "Polls",
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

        // Check that aggregation shows correct call counts and polls
        // attributed_no_log and attributed_with_log are each called 2 times
        // Each call has 2 polls, so total is 4 polls
        // The table should show "| 2     | 4     |" for call count and polls
        assert!(
            stdout.contains("| 2     | 4"),
            "Expected aggregated call count of 2 and poll count of 4.\nOutput:\n{}",
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

        // Test /tasks/{id}/logs endpoint
        let tasks_response: TasksJson =
            serde_json::from_str(&json_text).expect("Failed to parse tasks JSON");

        if let Some(first_task) = tasks_response.tasks.first() {
            // Get the first call's ID from the calls array
            if let Some(first_call) = first_task.task_calls.first() {
                let logs_url = format!("http://127.0.0.1:6775/tasks/{}/logs", first_call.id);
                let response = ureq::get(&logs_url)
                    .call()
                    .expect("Failed to call /tasks/{id}/logs endpoint");

                assert_eq!(
                    response.status(),
                    200,
                    "Expected status 200 for /tasks/{{id}}/logs endpoint"
                );
            }
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
