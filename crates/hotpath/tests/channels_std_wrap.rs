#[cfg(test)]
pub mod tests {
    use std::process::Command;

    #[cfg(feature = "hotpath")]
    use hotpath::json::{JsonChannelsList, JsonReport};

    // The report is followed by trailing log lines, so we locate the report's
    // opening brace and read just the first JSON value from that point.
    #[cfg(feature = "hotpath")]
    fn parse_channels(stdout: &str) -> JsonChannelsList {
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        report.channels.expect("No channels section in report")
    }

    #[cfg(feature = "hotpath")]
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

    // Bounded-std wrap requires `capacity`, which the macro accepts in any argument
    // position. Every order (including label/log before or after `wrap = true`) must
    // compile and register a wrapped channel.
    //
    // cargo run -p test-channels-std --example wrap_arg_orders_std --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_capacity_arg_orders() {
        let stdout = run_example("wrap_arg_orders_std");
        let channels = parse_channels(&stdout);

        for label in ["a", "b", "c", "d", "e", "f", "g", "h"] {
            let entry = channels
                .data
                .iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("channel {label:?} not found"));
            assert!(entry.wrap, "channel {label:?} should be endpoint-wrapped");
        }
    }

    // The self-tracked queue counter reports the exact depth (50 messages parked,
    // none received), where a forwarder proxy would drain immediately and report ~0.
    //
    // cargo run -p test-channels-std --example wrap_std --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_exact_queue_depth() {
        let stdout = run_example("wrap_std");
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
    // cargo run -p test-channels-std --example wrap_unbounded_std --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_unbounded_sent_received() {
        let stdout = run_example("wrap_unbounded_std");
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
    // panic the consumer thread and fail that check. Here we additionally assert the
    // counter never wrapped: a release-build underflow would surface as an absurd
    // queue length, so `received <= sent` and a bounded `max_queue_size` confirm sanity.
    //
    // cargo run -p test-channels-std --example wrap_concurrent_std --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_concurrent_no_underflow() {
        let stdout = run_example("wrap_concurrent_std");
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
    // closed. std receivers are not Clone, so there is no clone-count path.
    //
    // cargo run -p test-channels-std --example wrap_closed_std --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_receiver_dropped_closes() {
        let stdout = run_example("wrap_closed_std");
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
}
