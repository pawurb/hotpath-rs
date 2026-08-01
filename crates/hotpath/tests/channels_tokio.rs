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

    fn run_example(example: &str) -> String {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                example,
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}\nStderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    // cargo run -p test-channels-tokio --example agg_tokio --features hotpath
    #[test]
    fn test_default_mode_aggregates_per_callsite() {
        let stdout = run_example("agg_tokio");
        let channels = parse_channels(&stdout);

        assert_eq!(
            channels.data.len(),
            1,
            "default mode must aggregate all loop instances into one entry, got: {:?}",
            channels.data
        );
        let entry = &channels.data[0];
        assert_eq!(entry.instances, 5, "5 channels created at the call site");
        assert_eq!(entry.closed_instances, 5, "all endpoints dropped");
        assert_eq!(entry.state, "closed", "derived state after all closed");
        assert_eq!(entry.sent_count, 10, "summed across instances");
        assert_eq!(entry.received_count, 10, "summed across instances");
        assert_eq!(entry.iter, 0, "aggregated entries carry no iter suffix");

        // Rate sanity: throughput is total count over elapsed time since the
        // call site's first message. That window is at most the report's total
        // elapsed time, so the rate is bounded below by count / total elapsed.
        let rate = entry
            .sent_per_sec
            .expect("aggregated entry must report a rate");
        let elapsed_secs = channels.current_elapsed_ns as f64 / 1e9;
        let floor = entry.sent_count as f64 / elapsed_secs;
        assert!(
            rate >= floor * 0.9,
            "rate {rate} inconsistent with count over elapsed ({floor})"
        );
    }

    // cargo run -p test-channels-tokio --example agg_queue_tokio --features hotpath
    #[test]
    fn test_aggregated_queue_depth_is_combined() {
        let stdout = run_example("agg_queue_tokio");
        let channels = parse_channels(&stdout);

        assert_eq!(channels.data.len(), 1);
        let entry = &channels.data[0];
        assert_eq!(entry.instances, 2);
        // Two live instances each hold 3 messages: combined depth, not the
        // largest single-instance snapshot (3).
        assert_eq!(entry.queue_size, Some(6), "combined in-flight depth");
        assert!(
            entry.queue_size <= entry.max_queue_size,
            "current depth must stay within the tracked peak: {:?} > {:?}",
            entry.queue_size,
            entry.max_queue_size
        );
        assert_eq!(entry.state, "active", "instances still open at report time");
    }

    // cargo run -p test-channels-tokio --example agg_many_tokio --features hotpath
    #[test]
    fn test_default_mode_state_stays_bounded() {
        let stdout = run_example("agg_many_tokio");
        let channels = parse_channels(&stdout);

        // Boundedness: thousands of default-mode channels at one call site
        // must not register per-instance entries.
        assert_eq!(
            channels.data.len(),
            1,
            "expected a single aggregated entry for 2000 instances"
        );
        let entry = &channels.data[0];
        assert_eq!(entry.instances, 2000);
        assert_eq!(entry.state, "closed");
    }

    // cargo build -p test-channels-tokio --example iter_tokio
    #[test]
    fn test_iter_param_compiles_without_feature() {
        let output = Command::new("cargo")
            .args([
                "build",
                "-p",
                "test-channels-tokio",
                "--example",
                "iter_tokio",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "feature-off build of `channel!(..., iter = true)` failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // cargo run -p test-channels-tokio --example basic_tokio --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_tokio",
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
        let all_expected = [
            "Actor 1",
            "bounded-channel",
            "hello-there",
            "unbounded",
            "bounded[10]",
            "oneshot",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example basic_json_tokio --features hotpath
    #[test]
    fn test_basic_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_json_tokio",
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

        let sep = path_sep();
        let json_path = format!("\"label\": \"examples{sep}basic_json_tokio.rs:");
        let all_expected = [json_path.as_str(), "\"label\": \"hello-there\""];

        let stdout = String::from_utf8_lossy(&output.stdout);

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example closed_tokio --features hotpath
    #[test]
    fn test_closed_channels_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "closed_tokio",
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

    // cargo run -p test-channels-tokio --example oneshot_closed_tokio --features hotpath
    #[test]
    fn test_oneshot_closed_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "oneshot_closed_tokio",
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

        let all_expected = ["oneshot_closed_tokio.rs:"];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example iter_tokio --features hotpath
    #[test]
    fn test_iter_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "iter_tokio",
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
        let iter_39 = format!("examples{sep}iter_tokio.rs:39");
        let iter_39_2 = format!("examples{sep}iter_tokio.rs:39-2");
        let iter_39_3 = format!("examples{sep}iter_tokio.rs:39-3");
        let iter_51 = format!("examples{sep}iter_tokio.rs:51");
        let iter_51_2 = format!("examples{sep}iter_tokio.rs:51-2");
        let iter_51_3 = format!("examples{sep}iter_tokio.rs:51-3");
        let all_expected = [
            "Actor 1",
            "Actor 1-2",
            "Actor 1-3",
            iter_39.as_str(),
            iter_39_2.as_str(),
            iter_39_3.as_str(),
            iter_51.as_str(),
            iter_51_2.as_str(),
            iter_51_3.as_str(),
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-channels-tokio --example slow_consumer_tokio --features hotpath
    #[test]
    fn test_slow_consumer_no_panic() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "slow_consumer_tokio",
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

    // cargo run -p test-channels-tokio --example guard_timeout_channels --features hotpath
    #[test]
    fn test_guard_timeout_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "guard_timeout_channels",
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
        let expected_content = ["[hotpath]", "| channels", "timeout-channel"];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // HOTPATH_METRICS_PORT=6773 TEST_SLEEP_SECONDS=10 cargo run -p test-channels-tokio --example basic_tokio --features hotpath
    #[test]
    fn test_data_endpoints() {
        use hotpath::json::JsonChannelsList;
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_tokio",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6773")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut json_text = String::new();
        let mut last_error = None;

        let all_expected = ["basic_tokio.rs", "bounded-channel", "Actor 1"];

        for _attempt in 0..12 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6773/channels").call() {
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
            let logs_url = format!("http://localhost:6773/channels/{}/logs", channel.id);
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

    // HOTPATH_OUTPUT_FORMAT=none cargo run -p test-channels-tokio --example basic_tokio --features hotpath
    #[test]
    fn test_format_none_suppresses_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "basic_tokio",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "none")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            stdout.contains("Example completed!"),
            "Application output should still be present.\nGot:\n{stdout}"
        );

        let not_expected = [
            "[hotpath]",
            "bounded-channel",
            "hello-there",
            "Channel throughput",
        ];

        for not_exp in not_expected {
            assert!(
                !stdout.contains(not_exp),
                "Channel output should be suppressed with HOTPATH_OUTPUT_FORMAT=none.\nFound: {not_exp}\nGot:\n{stdout}"
            );
        }
    }

    // cargo run -p test-channels-tokio --example channels_file_output --features hotpath
    #[test]
    fn test_channels_file_output() {
        use std::fs;
        use std::path::Path;

        let output_path = "tmp/channels_output_test.json";

        fs::create_dir_all("tmp").ok();
        if Path::new(output_path).exists() {
            fs::remove_file(output_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-tokio",
                "--example",
                "channels_file_output",
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

        assert!(
            Path::new(output_path).exists(),
            "Output file was not created at {}",
            output_path
        );

        let file_content = fs::read_to_string(output_path).expect("Failed to read output file");

        let expected_content = ["test-channel", "\"sent_count\"", "\"received_count\""];

        for expected in expected_content {
            assert!(
                file_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{file_content}",
            );
        }

        fs::remove_file(output_path).ok();
    }
}
