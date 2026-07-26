//! Integration test for `io!` instrumentation over tokio async TCP. Runs the
//! `test-io` `basic_tokio_tcp_io` example (which spawns its own in-process
//! tokio echo server) as a subprocess and asserts on its report.
//! Self-contained - no external services required.
#[cfg(test)]
pub mod tests {
    #[cfg(feature = "hotpath")]
    use hotpath::json::JsonReport;
    use std::process::Command;

    #[cfg(feature = "hotpath")]
    #[test]
    fn test_tokio_tcp_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-io",
                "--example",
                "basic_tokio_tcp_io",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .output()
            .expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Command failed with status: {}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        let io = report.io.expect("No io section in report");

        let client = io
            .data
            .iter()
            .find(|e| e.label == "tokio-tcp-client")
            .expect("No 'tokio-tcp-client' entry in io section");

        assert!(client.type_name.contains("TcpStream"));
        // 8 rounds x 1024 bytes echoed each way; byte totals are exact while
        // the kernel may split individual reads or writes.
        assert!(client.write.count >= 8);
        assert_eq!(client.write.bytes, 8192);
        assert_eq!(client.write.errors, 0);
        assert!(client.read.count >= 8);
        assert_eq!(client.read.bytes, 8192);
        assert_eq!(client.read.errors, 0);
        assert!(client.read.total_ns > 0, "Reads should be timed");
        // Explicit AsyncWriteExt::shutdown call at the end of the example.
        assert_eq!(client.shutdown.count, 1);
        assert_eq!(client.shutdown.errors, 0);
    }
}
