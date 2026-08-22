#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;
    use std::{thread::sleep, time::Duration};

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
            .env("HOTPATH_METRICS_PORT", PORT)
            .env("HOTPATH_METRICS_AUTH_TOKEN", TOKEN)
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut ready = false;
        for _attempt in 0..40 {
            sleep(Duration::from_millis(750));
            if get_status(Some(TOKEN)).is_ok() {
                ready = true;
                break;
            }
        }
        if !ready {
            let _ = child.kill();
            panic!("Metrics server did not start on port {}", PORT);
        }

        let result = std::panic::catch_unwind(|| {
            let (status, body) = get_status(None).expect("request without token");
            assert_eq!(status, 401, "missing token, body: {body}");
            assert_eq!(body, r#"{"error":"Unauthorized"}"#);

            let (status, _) = get_status(Some("wrong-token")).expect("request with wrong token");
            assert_eq!(status, 401, "wrong token");

            let (status, body) = get_status(Some(TOKEN)).expect("request with correct token");
            assert_eq!(status, 200, "correct token, body: {body}");
            assert!(body.contains("uptime"), "unexpected status body: {body}");
        });

        let _ = child.kill();
        let _ = child.wait();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
