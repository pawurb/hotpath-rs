#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-tokio-async --example basic --features hotpath,hotpath-alloc
    #[test]
    fn test_basic_alloc_output() {
        for _ in 0..2 {
            let output = Command::new("cargo")
                .args([
                    "run",
                    "-p",
                    "test-tokio-async",
                    "--example",
                    "basic",
                    "--features",
                    "hotpath,hotpath-alloc",
                ])
                .env("HOTPATH_REPORT", "functions-alloc")
                .output()
                .expect("Failed to execute command");

            assert!(
                output.status.success(),
                "Process did not exit successfully.\n\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );

            let all_expected = [
                "custom_block",
                "basic::sync_function",
                "basic::async_function",
                "p95",
                "total",
                "percent_total",
            ];

            let stdout = String::from_utf8_lossy(&output.stdout);
            for expected in all_expected {
                assert!(
                    stdout.contains(expected),
                    "Expected:\n{expected}\n\nGot:\n{stdout}",
                );
            }
        }
    }

    // cargo run -p test-tokio-async --example early_returns --features hotpath,hotpath-alloc
    #[test]
    fn test_early_returns_alloc_output() {
        for _ in 0..2 {
            let output = Command::new("cargo")
                .args([
                    "run",
                    "-p",
                    "test-tokio-async",
                    "--example",
                    "early_returns",
                    "--features",
                    "hotpath,hotpath-alloc",
                ])
                .env("HOTPATH_REPORT", "functions-alloc")
                .output()
                .expect("Failed to execute command");

            assert!(
                output.status.success(),
                "Process did not exit successfully.\n\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );

            let all_expected = [
                "early_returns::early_return",
                "early_returns::propagates_error",
                "early_returns::normal_path",
            ];

            let stdout = String::from_utf8_lossy(&output.stdout);
            for expected in all_expected {
                assert!(
                    stdout.contains(expected),
                    "Expected:\n{expected}\n\nGot:\n{stdout}",
                );
            }
        }
    }

    // cargo run -p test-smol-async --example basic_smol --features hotpath,hotpath-alloc -- --nocapture
    #[test]
    fn test_async_smol_alloc_profiling_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-smol-async",
                "--example",
                "basic_smol",
                "--features",
                "hotpath,hotpath-alloc",
                "--",
                "--nocapture",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("basic_smol::main"),
            "Expected basic_smol::main in output\n\nGot:\n{stdout}",
        );
    }

    // cargo run -p test-tokio-async --example limit --features hotpath,hotpath-alloc
    #[test]
    fn test_limit_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "limit",
                "--features",
                "hotpath,hotpath-alloc",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let expected_content = [
            "(3/4)",
            "limit::main",
            "measured_module::function_one",
            "measured_module::function_two",
        ];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        let not_expected_content = ["limit::function_three", "N/A*"];

        for not_expected in not_expected_content {
            assert!(
                !stdout.contains(not_expected),
                "Not expected:\n{not_expected}\n\nGot:\n{stdout}"
            );
        }
    }

    // cargo run -p test-tokio-async --example multithread_alloc --features hotpath,hotpath-alloc
    #[test]
    fn test_multithread_alloc_no_panic() {
        let test_cases = [
            ("hotpath,hotpath-alloc", None),
            ("hotpath,hotpath-alloc", None),
            ("hotpath,hotpath-alloc", Some("true")),
            ("hotpath,hotpath-alloc", Some("true")),
        ];

        for (features, alloc_cumulative) in test_cases {
            let mut cmd = Command::new("cargo");
            cmd.args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "multithread_alloc",
                "--features",
                features,
            ]);
            cmd.env("HOTPATH_REPORT", "functions-alloc");

            if let Some(val) = alloc_cumulative {
                cmd.env("HOTPATH_ALLOC_CUMULATIVE", val);
            }

            let output = cmd.output().expect("Failed to execute command");

            let env_info = alloc_cumulative
                .map(|v| format!("HOTPATH_ALLOC_CUMULATIVE={}", v))
                .unwrap_or_else(|| "no env var".to_string());

            assert!(
                output.status.success(),
                "Process did not exit successfully with features: {}, {}\n\nstderr:\n{}",
                features,
                env_info,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // HOTPATH_METRICS_PORT=6775 TEST_SLEEP_SECONDS=10 cargo run -p test-tokio-async --example basic --features hotpath,hotpath-alloc
    #[test]
    fn test_data_endpoints() {
        use hotpath::json::JsonFunctionsList;
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath,hotpath-alloc",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .env("HOTPATH_METRICS_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        let mut timing_json = String::new();
        let mut last_error = None;

        let timing_expected = [
            "basic::sync_function",
            "basic::async_function",
            "custom_block",
        ];

        for _attempt in 0..18 {
            sleep(Duration::from_millis(750));

            match ureq::get("http://localhost:6775/functions_timing").call() {
                Ok(mut response) => {
                    timing_json = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    if timing_expected.iter().all(|e| timing_json.contains(e)) {
                        break;
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!(
                "Failed to connect to /functions_timing after 18 retries: {}",
                error
            );
        }

        for expected in timing_expected {
            assert!(
                timing_json.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{timing_json}",
            );
        }

        let timing_response: JsonFunctionsList =
            serde_json::from_str(&timing_json).expect("Failed to parse timing JSON");

        let mut alloc_response = ureq::get("http://localhost:6775/functions_alloc")
            .call()
            .expect("Failed to call /functions_alloc endpoint");

        assert_eq!(
            alloc_response.status(),
            200,
            "Expected status 200 for /functions_alloc endpoint"
        );

        let alloc_json = alloc_response
            .body_mut()
            .read_to_string()
            .expect("Failed to read alloc response body");

        for expected in timing_expected {
            assert!(
                alloc_json.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{alloc_json}",
            );
        }

        let _alloc_response: JsonFunctionsList =
            serde_json::from_str(&alloc_json).expect("Failed to parse alloc JSON");

        if let Some(first) = timing_response.data.first() {
            let function_id = first.id;

            let timing_logs_url = format!(
                "http://localhost:6775/functions_timing/{}/logs",
                function_id
            );
            let timing_logs_response = ureq::get(&timing_logs_url)
                .call()
                .expect("Failed to call /functions_timing/:id/logs endpoint");

            assert_eq!(
                timing_logs_response.status(),
                200,
                "Expected status 200 for /functions_timing/:id/logs endpoint"
            );

            let alloc_logs_url =
                format!("http://localhost:6775/functions_alloc/{}/logs", function_id);
            let alloc_logs_response = ureq::get(&alloc_logs_url)
                .call()
                .expect("Failed to call /functions_alloc/:id/logs endpoint");

            assert_eq!(
                alloc_logs_response.status(),
                200,
                "Expected status 200 for /functions_alloc/:id/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }

    // cargo run -p test-tokio-async --example basic --features hotpath,hotpath-alloc
    #[test]
    fn test_alloc_total_bytes_not_inflated() {
        use hotpath::json::JsonReport;

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath,hotpath-alloc",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let report: JsonReport = serde_json::from_str(stdout.lines().last().expect("no output"))
            .expect("Failed to parse JSON output");

        let alloc = report
            .functions_alloc
            .expect("Expected functions_alloc in report");

        let custom_block = alloc
            .data
            .iter()
            .find(|f| f.name == "custom_block")
            .expect("Expected custom_block in alloc data");

        assert_eq!(custom_block.calls, 100);

        let total_bytes =
            hotpath::parse_bytes(&custom_block.total).expect("Failed to parse custom_block total");
        assert!(
            total_bytes < 2048,
            "custom_block total should be under 2 KB, got {} B",
            total_bytes
        );
    }

    // cargo run -p test-tokio-async --example basic --features hotpath,hotpath-alloc
    #[test]
    fn test_async_alloc_is_reported() {
        use hotpath::json::JsonReport;

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath,hotpath-alloc",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .env("HOTPATH_METRICS_SERVER_OFF", "true")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let report: JsonReport = serde_json::from_str(stdout.lines().last().expect("no output"))
            .expect("Failed to parse JSON output");

        let alloc = report
            .functions_alloc
            .expect("Expected functions_alloc in report");

        let async_fn = alloc
            .data
            .iter()
            .find(|f| f.name == "basic::async_function")
            .expect("Expected basic::async_function in alloc data");

        assert_ne!(
            async_fn.total, "N/A",
            "async_function alloc should be reported when hotpath-alloc is enabled"
        );
    }

    // cargo run -p test-tokio-async --example alloc_measure --features hotpath,hotpath-alloc
    #[test]
    fn test_alloc_uninstrumented_children_tracked() {
        use hotpath::json::JsonReport;

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "alloc_measure",
                "--features",
                "hotpath,hotpath-alloc",
            ])
            .env("HOTPATH_REPORT", "functions-alloc")
            .env("HOTPATH_ALLOC_CUMULATIVE", "true")
            .env("HOTPATH_OUTPUT_FORMAT", "json")
            .env("HOTPATH_METRICS_SERVER_OFF", "true")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let report: JsonReport = serde_json::from_str(stdout.lines().last().expect("no output"))
            .expect("Failed to parse JSON output");

        let alloc = report
            .functions_alloc
            .expect("Expected functions_alloc in report");

        let find = |name: &str| -> &hotpath::json::JsonFunctionEntry {
            alloc
                .data
                .iter()
                .find(|f| f.name.ends_with(&format!("::{name}")))
                .unwrap_or_else(|| panic!("Expected {name} in alloc data"))
        };

        let assert_bytes = |name: &str, expected: u64| {
            let entry = find(name);
            let bytes = hotpath::parse_bytes(&entry.total)
                .unwrap_or_else(|| panic!("Failed to parse total for {name}: {}", entry.total));
            assert_eq!(
                bytes, expected,
                "{name}: expected {expected} B, got {bytes} B"
            );
        };

        assert_bytes("uninstrumented_children_2kb", 2048);
        assert_bytes("own_1kb_plus_uninstrumented_child_1kb", 2048);
        assert_bytes(
            "own_1kb_plus_uninstrumented_1kb_plus_instrumented_1kb",
            3072,
        );
        assert_bytes("instrumented_1kb", 1024);
    }
}
