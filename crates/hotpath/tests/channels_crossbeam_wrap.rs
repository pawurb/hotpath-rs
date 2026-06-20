#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-channels-crossbeam --example wrap_crossbeam --features hotpath
    #[cfg(feature = "hotpath")]
    #[test]
    fn test_wrap_exact_queue_depth() {
        use hotpath::json::JsonChannelsList;

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

        // The example emits a JSON report; extract it and assert the endpoint wrapper
        // reported the exact queue depth (50 messages parked, none received). A proxy
        // wrapper drains immediately and would report ~0 here.
        let json_start = stdout.find('{').expect("No JSON report in output");
        let json_text = &stdout[json_start..];
        // The report is followed by trailing log lines, so read just the first value.
        let report: serde_json::Value = serde_json::Deserializer::from_str(json_text)
            .into_iter::<serde_json::Value>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        let channels: JsonChannelsList =
            serde_json::from_value(report["channels"].clone()).expect("Failed to parse channels");

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
        use hotpath::json::JsonChannelsList;

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
        let json_start = stdout.find('{').expect("No JSON report in output");
        let json_text = &stdout[json_start..];
        let report: serde_json::Value = serde_json::Deserializer::from_str(json_text)
            .into_iter::<serde_json::Value>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        let channels: JsonChannelsList =
            serde_json::from_value(report["channels"].clone()).expect("Failed to parse channels");

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
