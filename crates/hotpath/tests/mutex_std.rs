#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-mutex-std --example basic_mutex_std --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-mutex-std",
                "--example",
                "basic_mutex_std",
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
            "Std Mutex example completed!",
            "mutexes",
            "counter",
            "Locks",
            "Wait avg",
            "Acq avg",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-mutex-std --example basic_mutex_std --features hotpath (json)
    #[test]
    fn test_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-mutex-std",
                "--example",
                "basic_mutex_std",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let all_expected = [
            "\"mutexes\"",
            "\"label\":\"counter\"",
            "\"count\":6",
            "\"wait_avg\"",
            "\"acquire_avg\"",
            "\"wait_percentiles\"",
            "\"acquire_percentiles\"",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // Locks recorded on a thread whose event queue drains ahead of the queue
    // carrying `Created` must still be counted (placeholder backfill).
    // cargo run -p test-mutex-std --example created_ordering --features hotpath (json)
    #[test]
    fn test_created_ordering_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-mutex-std",
                "--example",
                "created_ordering",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let all_expected = ["\"label\":\"target\"", "\"count\":100"];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }
}
