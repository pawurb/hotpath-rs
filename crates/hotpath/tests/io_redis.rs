//! Integration test for `io!` instrumentation over a real Redis TCP
//! connection. Runs the `test-io` `basic_redis_io` example as a subprocess and
//! asserts on its report. Requires the Redis container from the repo-root
//! compose file (`docker compose up -d redis`, host port 6390). Skips locally
//! when nothing listens there; on CI the redis service is mandatory, so a
//! missing server fails instead.
#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::JsonReport;
    use std::process::Command;

    fn redis_available() -> bool {
        let addr = "127.0.0.1:6390".parse().unwrap();
        let timeout = std::time::Duration::from_millis(500);
        std::net::TcpStream::connect_timeout(&addr, timeout).is_ok()
    }

    #[test]
    fn test_redis_json_output() {
        if !redis_available() {
            assert!(
                std::env::var_os("CI").is_none(),
                "no Redis on localhost:6390 - the CI redis service is required"
            );
            eprintln!(
                "skipping test_redis_json_output: no Redis on localhost:6390 \
                 (start it with `docker compose up -d redis`)"
            );
            return;
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-io",
                "--example",
                "basic_redis_io",
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

        let redis = io
            .data
            .iter()
            .find(|e| e.label == "redis")
            .expect("No 'redis' entry in io section");

        assert!(redis.type_name.contains("TcpStream"));
        // SET + GET + PING + DEL requests, one write each.
        assert_eq!(redis.write.count, 4);
        assert_eq!(redis.write.bytes, (49 + 31 + 14 + 31) as u64);
        assert_eq!(redis.write.errors, 0);
        // Four responses (+OK, $11\r\nhotpath-val\r\n, +PONG, :1); each needs
        // at least one read, but the kernel may split a response.
        assert!(redis.read.count >= 4);
        assert_eq!(redis.read.bytes, (5 + 18 + 7 + 4) as u64);
        assert_eq!(redis.read.errors, 0);
        assert!(redis.read.total_ns > 0, "Reads should be timed");
    }
}
