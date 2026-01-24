#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-metrics --example basic_metrics --features hotpath
    #[test]
    fn test_basic_metrics_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-metrics",
                "--example",
                "basic_metrics",
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

        assert!(
            stdout.contains("Hello, world!"),
            "Expected 'Hello, world!' in output, got:\n{}",
            stdout
        );
    }
}
