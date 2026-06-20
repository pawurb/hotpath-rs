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

    // cargo run -p test-channels-crossbeam --example wrap_crossbeam --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_exact_queue_depth() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "wrap_crossbeam",
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

        // The example emits a JSON report; assert the endpoint wrapper reported the
        // exact queue depth (50 messages parked, none received). A proxy wrapper
        // drains immediately and would report ~0 here.
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

    // cargo run -p test-channels-crossbeam --example wrap_closed_crossbeam --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_receiver_dropped_closes() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "wrap_closed_crossbeam",
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

        // Dropping the receiver while the sender is alive must mark the channel closed.
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

    // The last Receiver clone is dropped while Sender clones are still alive, and the
    // report is taken before any Sender is dropped. The endpoint wrapper must
    // still mark the channel closed.
    //
    // cargo run -p test-channels-crossbeam --example wrap_recv_clone_closed_crossbeam --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_receiver_clone_dropped_closes_with_sender_alive() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "wrap_recv_clone_closed_crossbeam",
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

        let channels = parse_channels(&stdout);

        let entry = channels
            .data
            .iter()
            .find(|c| c.label == "recv-clone-dropped")
            .expect("recv-clone-dropped channel not found");

        assert!(entry.wrap, "channel should be endpoint-wrapped");
        assert_eq!(
            entry.state, "closed",
            "expected closed state after all receivers dropped while senders alive"
        );
    }

    // cargo run -p test-channels-crossbeam --example wrap_latency_crossbeam --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_processing_histogram() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-channels-crossbeam",
                "--example",
                "wrap_latency_crossbeam",
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
        let channels = parse_channels(&stdout);

        // The configured percentiles are echoed at the top of the channels report.
        assert_eq!(channels.percentiles, vec![50.0, 95.0]);

        // Wrap channels carry an exact send->receive latency histogram in the JSON report.
        let wrap = channels
            .data
            .iter()
            .find(|c| c.label == "wrap-latency")
            .expect("wrap-latency channel not found");

        assert!(wrap.wrap, "channel should be endpoint-wrapped");
        let proc_avg = wrap
            .proc_avg
            .as_deref()
            .expect("wrap channel should report proc_avg in JSON");
        assert!(!proc_avg.is_empty(), "proc_avg should not be empty");
        assert_ne!(
            proc_avg, "0ns",
            "expected non-zero send->receive latency (~20ms held in channel)"
        );
        assert!(
            wrap.proc_percentiles.contains_key("p50"),
            "expected p50 latency percentile in JSON, got {:?}",
            wrap.proc_percentiles
        );
        assert!(
            wrap.proc_percentiles.contains_key("p95"),
            "expected p95 latency percentile in JSON, got {:?}",
            wrap.proc_percentiles
        );

        // Proxy (non-wrap) channels cannot measure latency accurately, so the
        // histogram fields are omitted rather than reported as zero.
        let proxy = channels
            .data
            .iter()
            .find(|c| c.label == "proxy-latency")
            .expect("proxy-latency channel not found");

        assert!(!proxy.wrap, "channel should be a proxy channel");
        assert!(
            proxy.proc_avg.is_none(),
            "proxy channel must not report proc_avg"
        );
        assert!(
            proxy.proc_percentiles.is_empty(),
            "proxy channel must not report latency percentiles"
        );
    }
}
