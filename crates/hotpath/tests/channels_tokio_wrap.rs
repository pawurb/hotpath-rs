#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    use hotpath::json::{JsonChannelsList, JsonReport};

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
                "test-channels-tokio",
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

    // The self-tracked queue counter reports the exact depth (50 messages parked,
    // none received), where a forwarder proxy would drain immediately and report ~0.
    // Tokio recovers bounded capacity from `max_capacity()`, so no `capacity` arg.
    //
    // cargo run -p test-channels-tokio --example wrap_tokio --features hotpath
    #[test]
    fn test_wrap_exact_queue_depth() {
        let stdout = run_example("wrap_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-queue")
            .expect("wrap-queue channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
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

    // Unbounded wrap: every message sent and drained, queue back to zero with the
    // high-water mark preserved.
    //
    // cargo run -p test-channels-tokio --example wrap_unbounded_tokio --features hotpath
    #[test]
    fn test_wrap_unbounded_sent_received() {
        let stdout = run_example("wrap_unbounded_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-unbounded")
            .expect("wrap-unbounded channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
        assert_eq!(entry.sent_count, 200, "expected 200 sends");
        assert_eq!(entry.received_count, 200, "expected 200 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
        assert_eq!(
            entry.max_queue_size,
            Some(200),
            "expected max queue depth of 200"
        );
    }

    // A producer racing a consumer on an unbounded wrap channel must never underflow
    // the depth counter (counting happens before each publish). `run_example` already
    // asserts the process exited successfully - in debug builds an underflow would
    // panic the consumer task and fail that check. Here we additionally assert the
    // counter never wrapped: a release-build underflow would surface as an absurd
    // queue length, so `received <= sent` and a bounded `max_queue_size` confirm sanity.
    //
    // cargo run -p test-channels-tokio --example wrap_concurrent_tokio --features hotpath
    #[test]
    fn test_wrap_concurrent_no_underflow() {
        let stdout = run_example("wrap_concurrent_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-concurrent")
            .expect("wrap-concurrent channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
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
    fn test_wrap_receiver_dropped_closes() {
        let stdout = run_example("wrap_closed_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "recv-dropped")
            .expect("recv-dropped channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
        assert_eq!(
            entry.state, "closed",
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
    fn test_wrap_weak_senders() {
        let stdout = run_example("weak_tokio");
        let channels = parse_channels(&stdout);

        for label in ["wrap-weak", "wrap-weak-unbounded"] {
            let entry = channels
                .data
                .iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("{label} channel not found"));

            assert!(entry.wrap, "channel should be endpoint-wrapped");
            assert_eq!(entry.sent_count, 2, "expected 2 sends on {label}");
            assert_eq!(entry.received_count, 2, "expected 2 receives on {label}");
            assert_eq!(
                entry.state, "closed",
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
    fn test_wrap_recv_many() {
        let stdout = run_example("recv_many_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-recv-many")
            .expect("wrap-recv-many channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
        assert_eq!(entry.sent_count, 60, "expected 60 sends");
        assert_eq!(entry.received_count, 60, "expected 60 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
        assert!(
            entry.proc_avg.is_some(),
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
    fn test_wrap_blocking_off_runtime() {
        let stdout = run_example("blocking_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-blocking")
            .expect("wrap-blocking channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
        assert_eq!(entry.sent_count, 25, "expected 25 sends");
        assert_eq!(entry.received_count, 25, "expected 25 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
    }

    // A timed-out send_timeout on a full channel rolls the depth counter back:
    // the failed send is not counted and the queue never exceeds capacity.
    //
    // cargo run -p test-channels-tokio --example send_timeout_tokio --features hotpath
    #[test]
    fn test_wrap_send_timeout_rollback() {
        let stdout = run_example("send_timeout_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-send-timeout")
            .expect("wrap-send-timeout channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
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
    fn test_wrap_poll_recv() {
        let stdout = run_example("poll_recv_tokio");
        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-poll-recv")
            .expect("wrap-poll-recv channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
        assert_eq!(entry.sent_count, 30, "expected 30 sends");
        assert_eq!(entry.received_count, 30, "expected 30 receives");
        assert_eq!(entry.queue_size, Some(0), "expected drained queue");
    }
}
