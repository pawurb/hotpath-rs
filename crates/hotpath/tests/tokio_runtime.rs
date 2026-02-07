#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    // HOTPATH_METRICS_PORT=6783 TEST_SLEEP_MS=5000 cargo run -p test-tokio-async --example tokio_runtime --features hotpath
    #[test]
    fn test_tokio_runtime_endpoint() {
        use hotpath::json::JsonRuntimeSnapshot;
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "tokio_runtime",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6783")
            .env("TEST_SLEEP_MS", "5000")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://localhost:6783/tokio_runtime").call() {
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
            panic!(
                "Failed to connect to /tokio_runtime after 12 retries: {}",
                error
            );
        }

        let snapshot: JsonRuntimeSnapshot =
            serde_json::from_str(&json_text).expect("Failed to parse runtime JSON");

        assert!(
            snapshot.num_workers > 0,
            "Expected at least 1 worker, got {}",
            snapshot.num_workers
        );
        assert_eq!(
            snapshot.workers.len(),
            snapshot.num_workers,
            "Workers array length should match num_workers"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    // HOTPATH_METRICS_PORT=6784 TEST_SLEEP_SECONDS=5 cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_tokio_runtime_404_without_init() {
        use std::{thread::sleep, time::Duration};

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
            .env("HOTPATH_METRICS_PORT", "6784")
            .env("TEST_SLEEP_SECONDS", "5")
            .spawn()
            .expect("Failed to spawn command");

        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build(),
        );

        let mut status = 0;
        let mut body = String::new();

        for _attempt in 0..12 {
            sleep(Duration::from_millis(500));

            match agent.get("http://localhost:6784/tokio_runtime").call() {
                Ok(mut resp) => {
                    status = resp.status().as_u16();
                    body = resp
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    break;
                }
                Err(_) => {}
            }
        }

        assert_eq!(status, 404, "Expected 404 status code");
        assert!(
            body.contains("error"),
            "Expected JSON error body, got: {}",
            body
        );
        assert!(
            body.contains("tokio_runtime!()"),
            "Expected guidance about tokio_runtime!(), got: {}",
            body
        );

        let _ = child.kill();
        let _ = child.wait();
    }
}
