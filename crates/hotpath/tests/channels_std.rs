#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    use hotpath::json::{JsonChannelsList, JsonReport};

    // cargo run -p test-channels-std --example basic_std --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                "basic_std",
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

        assert!(!output.stderr.is_empty(), "Stderr is empty");
        let all_expected = ["Actor 1", "unbounded-channel", "bounded[10]"];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-std --example basic_json_std --features hotpath
    #[test]
    fn test_basic_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                "basic_json_std",
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

        let all_expected = ["\"label\": \"unbounded\"", "\"label\": \"bounded\""];

        let stdout = String::from_utf8_lossy(&output.stdout);

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-std --example closed_std --features hotpath
    #[test]
    fn test_closed_channels_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                "closed_std",
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
    }

    // cargo run -p test-channels-std --example iter_std --features hotpath
    #[test]
    fn test_iter_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                "iter_std",
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
            "Actor 1",
            "Actor 1-2",
            "Actor 1-3",
            "bounded",
            "bounded-2",
            "bounded-3",
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-std --example slow_consumer_std --features hotpath
    #[test]
    fn test_slow_consumer_no_panic() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                "slow_consumer_std",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "Command failed with status: {}\nStdout:\n{}\nStderr:\n{}",
            output.status,
            stdout,
            stderr
        );

        assert!(
            stdout.contains("Slow consumer example completed!"),
            "Expected completion message not found.\nOutput:\n{}",
            stdout
        );
    }

    // HOTPATH_METRICS_PORT=6770 TEST_SLEEP_SECONDS=10 cargo run -p test-channels-std --example basic_std --features hotpath
    #[test]
    fn test_data_endpoints() {
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                "basic_std",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6770")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        let all_expected = ["basic_std.rs", "unbounded-channel", "Actor 1"];

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6770/channels").call() {
                Ok(mut response) => {
                    json_text = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    if all_expected.iter().all(|e| json_text.contains(e)) {
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
            panic!("Failed after 12 retries: {}", error);
        }

        for expected in all_expected {
            assert!(
                json_text.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{json_text}",
            );
        }

        let channels: JsonChannelsList =
            serde_json::from_str(&json_text).expect("Failed to parse channels JSON");

        if let Some(channel) = channels.data.first() {
            let logs_url = format!("http://localhost:6770/channels/{}/logs", channel.id);
            let response = ureq::get(&logs_url)
                .call()
                .expect("Failed to call /channels/:id/logs endpoint");

            assert_eq!(
                response.status(),
                200,
                "Expected status 200 for /channels/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }

    // Trailing log lines follow the report, so parse only the first JSON value.
    fn parse_channels(stdout: &str) -> JsonChannelsList {
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        report.channels.expect("No channels section in report")
    }

    fn run_example(name: &str) -> String {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-std",
                "--example",
                name,
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

        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    // Bounded std requires `capacity`, which the macro accepts in any argument
    // position. Every order of `capacity`/`label`/`log` must compile and register a channel.
    //
    // cargo run -p test-channels-std --example wrap_arg_orders_std --features hotpath
    #[test]
    fn test_capacity_arg_orders() {
        let stdout = run_example("wrap_arg_orders_std");
        let channels = parse_channels(&stdout);

        for label in ["a", "b", "c", "d", "e", "f", "g", "h"] {
            channels
                .data
                .iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("channel {label:?} not found"));
        }
    }

    // The self-tracked queue counter reports the exact depth (50 messages parked,
    // none received), where a forwarder would drain immediately and report ~0.
    //
    // cargo run -p test-channels-std --example wrap_std --features hotpath
    #[test]
    fn test_exact_queue_depth() {
        let stdout = run_example("wrap_std");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-queue")
            .expect("wrap-queue channel not found");
        assert_eq!(entry.sent_count, 50, "expected 50 sends");
        assert_eq!(
            entry.received_count, 0,
            "expected 0 receives at report time"
        );
        assert_eq!(
            entry.queue_size,
            Some(50),
            "expected exact queue depth of 50"
        );
        assert_eq!(
            entry.max_queue_size,
            Some(50),
            "expected max queue depth of 50"
        );
    }

    // Unbounded: every message sent and drained, queue back to zero with the
    // high-water mark preserved.
    //
    // cargo run -p test-channels-std --example wrap_unbounded_std --features hotpath
    #[test]
    fn test_unbounded_sent_received() {
        let stdout = run_example("wrap_unbounded_std");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-unbounded")
            .expect("wrap-unbounded channel not found");
        assert_eq!(entry.sent_count, 200, "expected 200 sends");
        assert_eq!(entry.received_count, 200, "expected 200 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
        assert_eq!(
            entry.max_queue_size,
            Some(200),
            "expected max queue depth of 200"
        );
    }

    // A producer racing a consumer must never underflow the depth counter: debug
    // builds would panic the consumer (caught by `run_example`), release builds
    // would wrap to an absurd queue length.
    //
    // cargo run -p test-channels-std --example wrap_concurrent_std --features hotpath
    #[test]
    fn test_concurrent_no_underflow() {
        let stdout = run_example("wrap_concurrent_std");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-concurrent")
            .expect("wrap-concurrent channel not found");
        assert!(
            entry.received_count <= entry.sent_count,
            "received ({}) must not exceed sent ({})",
            entry.received_count,
            entry.sent_count
        );
        assert!(
            entry.max_queue_size.unwrap_or(0) <= entry.sent_count as usize,
            "max queue ({:?}) is absurd - the depth counter underflowed and wrapped",
            entry.max_queue_size
        );
    }

    // Dropping the single receiver while the sender is alive must mark the channel
    // closed. std receivers are not Clone, so there is no clone-count path.
    //
    // cargo run -p test-channels-std --example wrap_closed_std --features hotpath
    #[test]
    fn test_receiver_dropped_closes() {
        let stdout = run_example("wrap_closed_std");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "recv-dropped")
            .expect("recv-dropped channel not found");
        assert_eq!(
            entry.state.as_deref(),
            Some("closed"),
            "expected closed state after receiver drop"
        );
    }
}
