//! Integration tests for `io!` instrumentation. Covers the self-contained
//! cases: sync file I/O (Cursor/BufReader/error readers), async tokio file and
//! duplex I/O, and std/tokio TCP against in-process echo servers. The
//! service-dependent Redis test lives in `io_redis.rs`.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use std::process::Command;

    use hotpath::json::{JsonIoEntry, JsonIoList, JsonReport};

    fn run_example(example: &str, json: bool) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-io",
            "--example",
            example,
            "--features",
            "hotpath",
        ]);
        if json {
            cmd.env("HOTPATH_OUTPUT_FORMAT", "json");
        }
        let output = cmd.output().expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn parse_io(stdout: &str) -> JsonIoList {
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        report.io.expect("No io section in report")
    }

    fn entry<'a>(io: &'a JsonIoList, label: &str) -> &'a JsonIoEntry {
        io.data
            .iter()
            .find(|e| e.label == label)
            .unwrap_or_else(|| panic!("No '{label}' entry in io section"))
    }

    // cargo run -p test-io --example basic_io_sync --features hotpath (json)
    #[test]
    fn test_sync_json_output() {
        let stdout = run_example("basic_io_sync", true);
        let io = parse_io(&stdout);

        let writer = entry(&io, "fixture-write");
        assert_eq!(writer.write.count, 10);
        assert_eq!(writer.write.bytes, 100);
        assert_eq!(writer.flush.count, 1);
        assert_eq!(writer.write.errors, 0);
        assert!(writer.type_name.contains("File"));

        let reader = entry(&io, "fixture-read");
        assert_eq!(reader.read.count, 10);
        assert_eq!(reader.read.bytes, 100);
        assert_eq!(reader.read.sampled_count, 10);
        assert!(reader.read.total_ns > 0, "Sync reads should be timed");
        assert_eq!(reader.read.errors, 0);
        assert_eq!(reader.write.count, 0);
        assert!(reader.type_name.contains("File"));

        // Delegation through BufReader: all bytes arrive, EOF read included.
        let buffered = entry(&io, "buffered");
        assert_eq!(buffered.read.bytes, 100);
        assert!(buffered.read.count >= 2);

        let failing = entry(&io, "failing");
        assert_eq!(failing.read.errors, 1);
        assert_eq!(failing.read.count, 0);

        // Retryable WouldBlock produces neither an operation nor an error.
        let busy = entry(&io, "busy");
        assert_eq!(busy.read.count, 0);
        assert_eq!(busy.read.errors, 0);

        // Flush failures are recorded as flush errors, not write errors.
        let flush_fail = entry(&io, "flush-fail");
        assert_eq!(flush_fail.write.count, 1);
        assert_eq!(flush_fail.write.errors, 0);
        assert_eq!(flush_fail.flush.count, 0);
        assert_eq!(flush_fail.flush.errors, 1);
    }

    // cargo run -p test-io --example basic_io_async --features hotpath (json)
    #[test]
    fn test_async_json_output() {
        let stdout = run_example("basic_io_async", true);
        let io = parse_io(&stdout);

        let writer = entry(&io, "fixture-write");
        assert_eq!(writer.write.count, 10);
        assert_eq!(writer.write.bytes, 100);
        assert_eq!(writer.flush.count, 1);
        assert_eq!(writer.shutdown.count, 1);
        assert_eq!(writer.write.errors, 0);

        // read_exact may split an operation on a partial fill, so the count is
        // a lower bound while the byte total stays exact.
        let reader = entry(&io, "fixture-read");
        assert!(reader.read.count >= 10);
        assert_eq!(reader.read.bytes, 100);
        assert_eq!(reader.read.errors, 0);

        let client = entry(&io, "duplex-client");
        assert_eq!(client.write.count, 1);
        assert_eq!(client.write.bytes, 5);
        assert_eq!(client.write.errors, 0);
        assert_eq!(client.flush.count, 1);
        assert_eq!(client.shutdown.count, 1);

        // The server delays its response by 200ms; the read spans Pending
        // polls, so its Pending-to-Ready duration must include that wait.
        assert!(client.read.count >= 1);
        assert_eq!(client.read.bytes, 10);
        assert!(
            client.read.total_ns >= 150_000_000,
            "Read duration should include async waiting time, got {}ns",
            client.read.total_ns
        );

        let err_reader = entry(&io, "err-reader");
        assert_eq!(err_reader.read.errors, 1);
        assert_eq!(err_reader.read.count, 0);

        // Retryable Interrupted counts as neither an op nor an error; the
        // retried read is recorded as its own single operation.
        let flaky = entry(&io, "flaky-reader");
        assert_eq!(flaky.read.errors, 0);
        assert_eq!(flaky.read.count, 1);
        assert_eq!(flaky.read.bytes, 5);
    }

    // cargo run -p test-io --example basic_tcp_io --features hotpath (json)
    #[test]
    fn test_tcp_json_output() {
        let stdout = run_example("basic_tcp_io", true);
        let io = parse_io(&stdout);
        let client = entry(&io, "tcp-client");

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
    }

    // cargo run -p test-io --example basic_tokio_tcp_io --features hotpath (json)
    #[test]
    fn test_tokio_tcp_json_output() {
        let stdout = run_example("basic_tokio_tcp_io", true);
        let io = parse_io(&stdout);
        let client = entry(&io, "tokio-tcp-client");

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

    // cargo run -p test-io --example basic_io_sync --features hotpath
    #[test]
    fn test_table_output() {
        let stdout = run_example("basic_io_sync", false);

        let all_expected = [
            "Sync io example completed!",
            "Byte-level I/O statistics",
            "fixture-write",
            "fixture-read",
            "Reads",
            "Writes",
            "Bytes",
            "Flushes",
            "Errors",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }
}
