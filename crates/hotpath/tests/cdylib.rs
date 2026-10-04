#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::process::Command;

    fn assert_success(output: std::process::Output, what: &str) {
        assert!(
            output.status.success(),
            "{what} failed.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // cargo build -p test-cdylib --lib --features hotpath
    #[test]
    fn test_cdylib_with_literal_labels_links() {
        let output = Command::new("cargo")
            .args([
                "build",
                "-p",
                "test-cdylib",
                "--lib",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute cargo build");
        assert_success(output, "cdylib build");
    }

    // cargo run -p test-cdylib-dep --example cross_crate_labels --features hotpath
    #[test]
    fn test_same_label_in_two_crates_links() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-cdylib-dep",
                "--example",
                "cross_crate_labels",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute cargo run");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert_success(output, "cross_crate_labels example");
        assert!(stdout.contains("total: 21"), "unexpected output:\n{stdout}");
    }
}
