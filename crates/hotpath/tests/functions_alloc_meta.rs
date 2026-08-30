#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use hotpath::json::JsonReport;
    use std::process::Command;

    // cargo run -p test-tokio-async --example basic --features hotpath,hotpath-alloc,hotpath-meta,hotpath-alloc-meta
    #[test]
    fn test_combined_alloc_and_alloc_meta_output() {
        let meta_output = std::env::temp_dir().join("hotpath_functions_alloc_meta_test.txt");
        let _ = std::fs::remove_file(&meta_output);

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath,hotpath-alloc,hotpath-meta,hotpath-alloc-meta",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .env("HOTPATH_META_REPORT", "functions-alloc")
            .env("HOTPATH_META_OUTPUT_PATH", &meta_output)
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");

        let functions_alloc = report
            .functions_alloc
            .expect("No functions_alloc section in report");
        assert!(!functions_alloc.data.is_empty());
        let total_allocated = functions_alloc
            .total_allocated
            .expect("No total_allocated in functions_alloc section");
        assert_ne!(total_allocated, "0 B", "User-level alloc tracking is dead");

        let meta_report =
            std::fs::read_to_string(&meta_output).expect("Meta report file was not written");
        assert!(
            meta_report.contains("alloc-bytes"),
            "Expected alloc-bytes section in meta report:\n{meta_report}"
        );
        let total_line = meta_report
            .lines()
            .find(|line| line.starts_with("Total:"))
            .expect("No Total line in meta report");
        assert_ne!(
            total_line.trim(),
            "Total: 0 B",
            "Meta alloc tracking is dead:\n{meta_report}"
        );

        let _ = std::fs::remove_file(&meta_output);
    }
}
