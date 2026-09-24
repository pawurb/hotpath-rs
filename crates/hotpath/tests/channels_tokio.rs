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
        assert_eq!(entry.state, None, "aggregated entries report no state");
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
        assert_eq!(entry.state, None, "aggregated entries report no state");
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
        assert_eq!(entry.state, None);
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
        let all_expected = [
            json_path.as_str(),
            "\"label\": \"hello-there\"",
            "\"state\": \"notified\"",
        ];

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
        let iter_41 = format!("examples{sep}iter_tokio.rs:41");
        let iter_41_2 = format!("examples{sep}iter_tokio.rs:41-2");
        let iter_41_3 = format!("examples{sep}iter_tokio.rs:41-3");
        let iter_53 = format!("examples{sep}iter_tokio.rs:53");
        let iter_53_2 = format!("examples{sep}iter_tokio.rs:53-2");
        let iter_53_3 = format!("examples{sep}iter_tokio.rs:53-3");
        let all_expected = [
            "Actor 1",
            "Actor 1-2",
            "Actor 1-3",
            iter_41.as_str(),
            iter_41_2.as_str(),
            iter_41_3.as_str(),
            iter_53.as_str(),
            iter_53_2.as_str(),
            iter_53_3.as_str(),
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

    // The self-tracked queue counter reports the exact depth (50 messages parked,
    // none received), where a forwarder would drain immediately and report ~0.
    // Tokio recovers bounded capacity from `max_capacity()`, so no `capacity` arg.
    //
    // cargo run -p test-channels-tokio --example wrap_tokio --features hotpath
    #[test]
    fn test_exact_queue_depth() {
        let stdout = run_example("wrap_tokio");
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
    // cargo run -p test-channels-tokio --example wrap_unbounded_tokio --features hotpath
    #[test]
    fn test_unbounded_sent_received() {
        let stdout = run_example("wrap_unbounded_tokio");
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

    // A producer racing a consumer on an unbounded channel must never underflow
    // the depth counter (counting happens before each publish). `run_example` already
    // asserts the process exited successfully - in debug builds an underflow would
    // panic the consumer task and fail that check. Here we additionally assert the
    // counter never wrapped: a release-build underflow would surface as an absurd
    // queue length, so `received <= sent` and a bounded `max_queue_size` confirm sanity.
    //
    // cargo run -p test-channels-tokio --example wrap_concurrent_tokio --features hotpath
    #[test]
    fn test_concurrent_no_underflow() {
        let stdout = run_example("wrap_concurrent_tokio");
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
    // closed. tokio receivers are not Clone, so there is no clone-count path.
    //
    // cargo run -p test-channels-tokio --example wrap_closed_tokio --features hotpath
    #[test]
    fn test_receiver_dropped_closes() {
        let stdout = run_example("wrap_closed_tokio");
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

    // Weak senders: the example asserts the downgrade/upgrade lifecycle in-process
    // (upgrade fails after all strong senders drop, wrapper counts match the
    // receiver-side counts); here we assert the report sees the upgraded sender's
    // traffic and exactly one Closed transition (state is terminal-closed, counters
    // intact).
    //
    // cargo run -p test-channels-tokio --example weak_tokio --features hotpath
    #[test]
    fn test_weak_senders() {
        let stdout = run_example("weak_tokio");
        let channels = parse_channels(&stdout);

        for label in ["wrap-weak", "wrap-weak-unbounded"] {
            let entry = channels
                .data
                .iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("{label} channel not found"));
            assert_eq!(entry.sent_count, 2, "expected 2 sends on {label}");
            assert_eq!(entry.received_count, 2, "expected 2 receives on {label}");
            assert_eq!(
                entry.state.as_deref(),
                Some("closed"),
                "expected closed state after all strong senders dropped on {label}"
            );
        }
    }

    // Batch receive: recv_many drains in chunks but every message gets its own
    // receive event, so counts are exact, the queue returns to zero, and the delay
    // histogram is populated.
    //
    // cargo run -p test-channels-tokio --example recv_many_tokio --features hotpath
    #[test]
    fn test_recv_many() {
        let stdout = run_example("recv_many_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-recv-many")
            .expect("wrap-recv-many channel not found");
        assert_eq!(entry.sent_count, 60, "expected 60 sends");
        assert_eq!(entry.received_count, 60, "expected 60 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
        assert!(
            entry.delay_avg.is_some(),
            "expected populated delay histogram"
        );

        let unbounded = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-recv-many-unbounded")
            .expect("wrap-recv-many-unbounded channel not found");
        assert_eq!(unbounded.sent_count, 30, "expected 30 sends");
        assert_eq!(unbounded.received_count, 30, "expected 30 receives");
        assert_eq!(unbounded.queue_size, Some(0), "expected drained queue");
    }

    // Blocking variants driven from std threads without a runtime: stats recorded,
    // no panic (the example itself asserts message ordering).
    //
    // cargo run -p test-channels-tokio --example blocking_tokio --features hotpath
    #[test]
    fn test_blocking_off_runtime() {
        let stdout = run_example("blocking_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-blocking")
            .expect("wrap-blocking channel not found");
        assert_eq!(entry.sent_count, 25, "expected 25 sends");
        assert_eq!(entry.received_count, 25, "expected 25 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
    }

    // A timed-out send_timeout on a full channel rolls the depth counter back:
    // the failed send is not counted and the queue never exceeds capacity.
    //
    // cargo run -p test-channels-tokio --example send_timeout_tokio --features hotpath
    #[test]
    fn test_send_timeout_rollback() {
        let stdout = run_example("send_timeout_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-send-timeout")
            .expect("wrap-send-timeout channel not found");
        assert_eq!(entry.sent_count, 5, "timed-out send must not be counted");
        assert_eq!(
            entry.queue_size,
            Some(5),
            "queue must not exceed capacity after rollback"
        );
        assert_eq!(
            entry.max_queue_size,
            Some(5),
            "max queue must not exceed capacity after rollback"
        );
    }

    // Manual polling via poll_recv/poll_recv_many records every receive; the
    // example exercises a Pending-then-Ready sequence on the reusable scratch buffer.
    //
    // cargo run -p test-channels-tokio --example poll_recv_tokio --features hotpath
    #[test]
    fn test_poll_recv() {
        let stdout = run_example("poll_recv_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-poll-recv")
            .expect("wrap-poll-recv channel not found");
        assert_eq!(entry.sent_count, 30, "expected 30 sends");
        assert_eq!(entry.received_count, 30, "expected 30 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
    }

    // Two `channel!` invocations on one physical line (same message type) must
    // register distinct entries: the registration key includes the column, so
    // the second call site does not reuse the first one's id. The displayed
    // source keeps the plain `file:line` form.
    //
    // cargo run -p test-channels-tokio --example same_line_tokio --features hotpath
    #[test]
    fn test_same_line_call_sites_stay_distinct() {
        let stdout = run_example("same_line_tokio");
        let channels = parse_channels(&stdout);

        let a = channels
            .data
            .iter()
            .find(|c| c.label == "same-line-a")
            .expect("same-line-a channel not found");
        let b = channels
            .data
            .iter()
            .find(|c| c.label == "same-line-b")
            .expect("same-line-b channel not found");

        assert_ne!(a.id, b.id, "same-line call sites must not share an entry");
        assert_eq!(a.sent_count, 3, "counts must not merge across call sites");
        assert_eq!(b.sent_count, 5, "counts must not merge across call sites");
        assert_eq!(a.channel_type, "bounded[4]");
        assert_eq!(b.channel_type, "bounded[8]");
        assert_eq!(a.instances, 1);
        assert_eq!(b.instances, 1);

        // The displayed source is identical for both (same file:line) and the
        // path contains no ':', so exactly one colon proves the column stays
        // out of the display string.
        assert_eq!(a.source, b.source, "one physical line renders one source");
        assert_eq!(
            a.source.matches(':').count(),
            1,
            "source must stay file:line"
        );
    }
}
