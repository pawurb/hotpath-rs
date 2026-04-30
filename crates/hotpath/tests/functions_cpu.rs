#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-tokio-async --example cpu_basic --features hotpath,hotpath-cpu
    #[test]
    fn test_cpu_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "cpu_basic",
                "--features",
                "hotpath,hotpath-cpu",
            ])
            .env("HOTPATH_REPORT", "functions-cpu")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let expected_content = ["cpu_basic::heavy_work", "cpu_basic::light_work"];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // HOTPATH_OUTPUT_FORMAT=json HOTPATH_REPORT=functions-cpu cargo run -p test-tokio-async --example cpu_symbols --features hotpath,hotpath-cpu
    #[test]
    fn test_cpu_symbols_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "cpu_symbols",
                "--features",
                "hotpath,hotpath-cpu",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .env("HOTPATH_REPORT", "functions-cpu")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_line = stdout.lines().last().expect("no output");

        let expected_symbols = [
            "cpu_symbols::free_heavy_work",
            "Worker::method_heavy_work",
            "Worker::method_light_work",
            "OtherWorker::method_heavy_work",
        ];

        for expected in expected_symbols {
            assert!(
                json_line.contains(expected),
                "Expected symbol:\n{expected}\n\nGot:\n{json_line}",
            );
        }
    }
}
