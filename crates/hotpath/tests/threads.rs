#[cfg(test)]
pub mod tests {
    use hotpath::threads::ThreadsJson;
    use std::process::Command;
    use std::thread::sleep;
    use std::time::Duration;

    // HOTPATH_METRICS_PORT=6775 TEST_SLEEP_SECONDS=10 cargo run -p test-tokio-async --example basic --features hotpath
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
                    break;
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

        // Parse JSON response
        let threads_response: ThreadsJson =
            serde_json::from_str(&json_text).expect("Failed to parse threads JSON");

        // Assert we have at least some threads
        assert!(
            threads_response.thread_count > 0,
            "Expected at least 1 thread, got {}",
            threads_response.thread_count
        );

        assert_eq!(
            threads_response.thread_count,
            threads_response.threads.len(),
            "thread_count should match threads.len()"
        );

        let hp_threads: Vec<_> = threads_response
            .threads
            .iter()
            .filter(|t| t.name.starts_with("hp-"))
            .collect();

        assert!(
            !hp_threads.is_empty(),
            "Expected at least one hp- thread, found none. Threads: {:?}",
            threads_response
                .threads
                .iter()
                .map(|t| &t.name)
                .collect::<Vec<_>>()
        );

        for thread in &threads_response.threads {
            assert!(thread.os_tid > 0, "Thread should have valid os_tid");
            assert!(thread.cpu_total >= 0.0, "CPU total should be non-negative");
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
