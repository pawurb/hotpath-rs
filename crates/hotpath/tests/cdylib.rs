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

    fn run_dep_target(target: &[&str]) {
        let output = Command::new("cargo")
            .args(["run", "-p", "test-cdylib-dep"])
            .args(target)
            .args(["--features", "hotpath"])
            .output()
            .expect("Failed to execute cargo run");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert_success(output, &target.join(" "));
        assert!(stdout.contains("total: 21"), "unexpected output:\n{stdout}");
    }

    // cargo run -p test-cdylib-dep --example cross_crate_labels --features hotpath
    #[test]
    fn test_same_label_in_two_crates_links() {
        run_dep_target(&["--example", "cross_crate_labels"]);
    }

    // cargo run -p test-cdylib-dep --bin test-cdylib-dep --features hotpath
    #[test]
    fn test_same_label_in_lib_and_bin_of_one_package_links() {
        run_dep_target(&["--bin", "test-cdylib-dep"]);
    }

    // cargo test -p test-cdylib-dep --features hotpath --test test_cdylib_dep
    #[test]
    fn test_same_label_in_lib_and_same_named_integration_test_links() {
        let output = Command::new("cargo")
            .args([
                "test",
                "-p",
                "test-cdylib-dep",
                "--features",
                "hotpath",
                "--test",
                "test_cdylib_dep",
            ])
            .output()
            .expect("Failed to execute cargo test");
        assert_success(output, "test_cdylib_dep integration test");
    }
}
