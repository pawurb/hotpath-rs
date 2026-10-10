#[path = "common/support.rs"]
mod common;

#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::{thread::sleep, time::Duration};

    use crate::common::example::Example;

    const PORT: &str = "6788";
    const TOKEN: &str = "test-secret";

    fn get_status(auth: Option<&str>) -> Result<(u16, String), ureq::Error> {
        let url = format!("http://localhost:{}/profiler_status", PORT);
        let mut request = ureq::get(&url).config().http_status_as_error(false).build();
        if let Some(token) = auth {
            request = request.header("Authorization", token);
        }
        let mut response = request.call()?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string()?;
        Ok((status, body))
    }

    // cargo run -p test-channels-crossbeam --example basic_crossbeam --features hotpath
    #[test]
    fn test_metrics_auth_token() {
        let _running = Example::new("test-channels-crossbeam", "basic_crossbeam")
            .env("HOTPATH_METRICS_PORT", PORT)
            .env("HOTPATH_METRICS_AUTH_TOKEN", TOKEN)
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn();

        let mut ready = false;
        for _attempt in 0..40 {
            sleep(Duration::from_millis(750));
            if get_status(Some(TOKEN)).is_ok() {
                ready = true;
                break;
            }
        }
        assert!(ready, "Metrics server did not start on port {}", PORT);

        let (status, body) = get_status(None).expect("request without token");
        assert_eq!(status, 401, "missing token, body: {body}");
        assert_eq!(body, r#"{"error":"Unauthorized"}"#);

        let (status, _) = get_status(Some("wrong-token")).expect("request with wrong token");
        assert_eq!(status, 401, "wrong token");

        let (status, body) = get_status(Some(TOKEN)).expect("request with correct token");
        assert_eq!(status, 200, "correct token, body: {body}");
        assert!(body.contains("uptime"), "unexpected status body: {body}");
    }
}
