#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    use hotpath::json::{JsonChannelsList, JsonReport};

    fn path_sep() -> &'static str {
        if cfg!(windows) {
            "\\"
        } else {
            "/"
        }
    }

    // cargo run -p test-channels-flume --example basic_flume --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-flume",
                "--example",
                "basic_flume",
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
        let all_expected = ["Actor 1", "bounded-channel", "unbounded", "bounded[10]"];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-flume --example basic_json_flume --features hotpath
    #[test]
    fn test_basic_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-flume",
                "--example",
                "basic_json_flume",
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

    // cargo run -p test-channels-flume --example closed_flume --features hotpath
    #[test]
    fn test_closed_channels_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-flume",
                "--example",
                "closed_flume",
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

    // cargo run -p test-channels-flume --example iter_flume --features hotpath
    #[test]
    fn test_iter_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-flume",
                "--example",
                "iter_flume",
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

        let sep = path_sep();
        let iter_27 = format!("examples{sep}iter_flume.rs:27");
        let iter_27_2 = format!("examples{sep}iter_flume.rs:27-2");
        let iter_27_3 = format!("examples{sep}iter_flume.rs:27-3");
        let iter_40 = format!("examples{sep}iter_flume.rs:40");
        let iter_40_2 = format!("examples{sep}iter_flume.rs:40-2");
        let iter_40_3 = format!("examples{sep}iter_flume.rs:40-3");
        let all_expected = [
            iter_27.as_str(),
            iter_27_2.as_str(),
            iter_27_3.as_str(),
            iter_40.as_str(),
            iter_40_2.as_str(),
            iter_40_3.as_str(),
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-flume --example slow_consumer_flume --features hotpath
    #[test]
    fn test_slow_consumer_no_panic() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-flume",
                "--example",
                "slow_consumer_flume",
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

    // HOTPATH_METRICS_PORT=6772 TEST_SLEEP_SECONDS=10 cargo run -p test-channels-flume --example basic_flume --features hotpath
    #[test]
    fn test_data_endpoints() {
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-flume",
                "--example",
                "basic_flume",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6772")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        let all_expected = ["basic_flume.rs", "bounded-channel", "Actor 1"];

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6772/channels").call() {
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
            let logs_url = format!("http://localhost:6772/channels/{}/logs", channel.id);
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

    // The report is followed by trailing log lines, so we locate the report's
    // opening brace and read just the first JSON value from that point.
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
                "test-channels-flume",
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

    // The endpoint wrapper samples the real channel length, so it reports the exact
    // depth (50 messages parked, none received). A forwarder would drain immediately
    // and would report ~0 here.
    //
    // cargo run -p test-channels-flume --example wrap_flume --features hotpath
    #[test]
    fn test_exact_queue_depth() {
        let stdout = run_example("wrap_flume");
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

    // Dropping the receiver while the sender is alive must mark the channel closed.
    //
    // cargo run -p test-channels-flume --example wrap_closed_flume --features hotpath
    #[test]
    fn test_receiver_dropped_closes() {
        let stdout = run_example("wrap_closed_flume");
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

    // The last Receiver clone is dropped while Sender clones are still alive, and the
    // report is taken before any Sender is dropped. The endpoint wrapper must still
    // mark the channel closed.
    //
    // cargo run -p test-channels-flume --example wrap_recv_clone_closed_flume --features hotpath
    #[test]
    fn test_receiver_clone_dropped_closes_with_sender_alive() {
        let stdout = run_example("wrap_recv_clone_closed_flume");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "recv-clone-dropped")
            .expect("recv-clone-dropped channel not found");
        assert_eq!(
            entry.state.as_deref(),
            Some("closed"),
            "expected closed state after all receivers dropped while senders alive"
        );
    }

    // cargo run -p test-channels-flume --example wrap_latency_flume --features hotpath
    #[test]
    fn test_processing_histogram() {
        let stdout = run_example("wrap_latency_flume");
        let channels = parse_channels(&stdout);

        // The configured percentiles are echoed at the top of the channels report.
        assert_eq!(channels.percentiles, vec![50.0, 95.0]);

        // Channels carry an exact send->receive latency histogram in the JSON report.
        let latency = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-latency")
            .expect("wrap-latency channel not found");
        let delay_avg = latency
            .delay_avg
            .as_deref()
            .expect("channel should report delay_avg in JSON");
        assert!(!delay_avg.is_empty(), "delay_avg should not be empty");
        assert_ne!(
            delay_avg, "0ns",
            "expected non-zero send->receive latency (~20ms held in channel)"
        );
        assert!(
            latency.delay_percentiles.contains_key("p50"),
            "expected p50 latency percentile in JSON, got {:?}",
            latency.delay_percentiles
        );
        assert!(
            latency.delay_percentiles.contains_key("p95"),
            "expected p95 latency percentile in JSON, got {:?}",
            latency.delay_percentiles
        );
    }
}
