#[cfg(test)]
pub mod tests {
    use std::process::Command;

    #[test]
    fn test_basic_streams_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-streams",
                "--example",
                "basic_streams",
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

        let all_expected = [
            "number-stream",
            "text-stream",
            "repeat-stream",
            "Stream example completed!",
            "Streams:",
            "5", // number-stream yielded 5 items
            "4", // text-stream yielded 4 items
            "3", // repeat-stream yielded 3 items
            "Yielded",
        ];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_streams_closed_state() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-streams",
                "--example",
                "basic_streams",
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

        // All streams should be in closed state after completion
        let closed_count = stdout.matches("| closed").count();
        assert!(
            closed_count >= 3,
            "Expected at least 3 'closed' states for streams, found {}.\nOutput:\n{}",
            closed_count,
            stdout
        );
    }
}
