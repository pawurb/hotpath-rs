#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-rw-lock-std --example basic_rw_lock_std --features hotpath
    #[test]
    fn test_basic_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-rw-lock-std",
                "--example",
                "basic_rw_lock_std",
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
            "Std RwLock example completed!",
            "rw_locks",
            "counter",
            "Reads",
            "Writes",
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

    // cargo run -p test-rw-lock-std --example basic_rw_lock_std --features hotpath (json)
    #[test]
    fn test_json_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-rw-lock-std",
                "--example",
                "basic_rw_lock_std",
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
            "\"rw_locks\"",
            "\"label\":\"counter\"",
            "\"read_count\":6",
            "\"write_count\":3",
            "\"read_wait_avg\"",
            "\"write_wait_avg\"",
            "\"read_acquire_avg\"",
            "\"write_acquire_avg\"",
            "\"read_wait_percentiles\"",
            "\"write_wait_percentiles\"",
            "\"read_acquire_percentiles\"",
            "\"write_acquire_percentiles\"",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }
}
