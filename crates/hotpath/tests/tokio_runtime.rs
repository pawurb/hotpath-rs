#[path = "common/support.rs"]
mod common;

#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use crate::common::example::{poll_endpoint, Example};

    // HOTPATH_METRICS_PORT=6783 TEST_SLEEP_MS=5000 cargo run -p test-tokio-async --example tokio_runtime --features hotpath
    #[test]
    fn test_tokio_runtime_endpoint() {
        use hotpath::json::JsonRuntimeSnapshot;
        use std::time::Duration;

        let _running = Example::new("test-tokio-async", "tokio_runtime")
            .env("HOTPATH_METRICS_PORT", "6783")
            .env("TEST_SLEEP_MS", "5000")
            .spawn();

        let json_text = poll_endpoint(
            "http://localhost:6783/tokio_runtime",
            12,
            Duration::from_millis(750),
            |body| body.contains("num_workers") && !body.contains("\"num_workers\":0"),
        );

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
    }

    // HOTPATH_METRICS_PORT=6784 TEST_SLEEP_SECONDS=5 cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_tokio_runtime_404_without_init() {
        use std::{thread::sleep, time::Duration};

        let _running = Example::new("test-tokio-async", "basic")
            .env("HOTPATH_METRICS_PORT", "6784")
            .env("TEST_SLEEP_SECONDS", "5")
            .spawn();

        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build(),
        );

        let mut status = 0;
        let mut body = String::new();

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            if let Ok(mut resp) = agent.get("http://localhost:6784/tokio_runtime").call() {
                status = resp.status().as_u16();
                body = resp
                    .body_mut()
                    .read_to_string()
                    .expect("Failed to read response body");
                break;
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
    }
}
