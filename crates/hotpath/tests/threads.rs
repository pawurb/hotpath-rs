#[cfg(test)]
pub mod tests {
    #[cfg(feature = "hotpath")]
    use {hotpath::json::JsonThreadsList, std::thread::sleep, std::time::Duration};

    use std::process::Command;
    // HOTPATH_METRICS_PORT=6775 TEST_SLEEP_SECONDS=10 cargo run -p test-tokio-async --example basic --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_threads_endpoint() {
        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        // Test /threads endpoint
        for _attempt in 0..30 {
            sleep(Duration::from_millis(1000));

            match ureq::get("http://localhost:6775/threads").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    if json_text.contains("thread_count")
                        && !json_text.contains("\"thread_count\":0")
                    {
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
            panic!("Failed after 30 retries: {}", error);
        }

        // Parse JSON response (server returns formatted values)
        let threads_response: JsonThreadsList =
            serde_json::from_str(&json_text).expect("Failed to parse threads JSON");

        // Assert we have at least some threads
        assert!(
            threads_response.thread_count > 0,
            "Expected at least 1 thread, got {}",
            threads_response.thread_count
        );

        assert_eq!(
            threads_response.thread_count,
            threads_response.data.len(),
            "thread_count should match data.len()"
        );

        let hp_threads: Vec<_> = threads_response
            .data
            .iter()
            .filter(|t| t.name.starts_with("hp-"))
            .collect();

        assert!(
            !hp_threads.is_empty(),
            "Expected at least one hp- thread, found none. Threads: {:?}",
            threads_response
                .data
                .iter()
                .map(|t| &t.name)
                .collect::<Vec<_>>()
        );

        for thread in &threads_response.data {
            assert!(thread.os_tid > 0, "Thread should have valid os_tid");
        }

        let _ = child.kill();
        let _ = child.wait();
    }

    // cargo run -p test-tokio-async --example guard_timeout_threads --features hotpath
    #[test]
    fn test_guard_timeout_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "guard_timeout_threads",
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
            "| threads",
            "Thread CPU and memory statistics.",
            "RSS:",
        ];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }
}
