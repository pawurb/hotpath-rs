#[cfg(test)]
pub mod tests {
    use std::process::Command;

    // cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_basic_output() {
        let features = ["", "hotpath-alloc", "hotpath-alloc"];

        for feature in features {
            let features_arg = if feature.is_empty() {
                "hotpath".to_string()
            } else {
                format!("hotpath,{}", feature)
            };

            let output = Command::new("cargo")
                .args([
                    "run",
                    "-p",
                    "test-tokio-async",
                    "--example",
                    "basic",
                    "--features",
                    &features_arg,
                ])
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

    // cargo run -p test-tokio-async --example early_returns --features hotpath
    #[test]
    fn test_early_returns_output() {
        let features = ["hotpath", "hotpath-alloc", "hotpath-alloc"];
        for feature in features {
            let features_arg = if feature == "hotpath" {
                "hotpath".to_string()
            } else {
                format!("hotpath,{}", feature)
            };

            let output = Command::new("cargo")
                .args([
                    "run",
                    "-p",
                    "test-tokio-async",
                    "--example",
                    "early_returns",
                    "--features",
                    &features_arg,
                ])
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

    // cargo run -p test-tokio-async --example unsupported_async --features hotpath,hotpath-alloc
    #[test]
    fn test_unsupported_async_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "unsupported_async",
                "--features",
                "hotpath,hotpath-alloc",
            ])
            .output()
            .expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout);

        let all_expected = ["N/A*", "only available for tokio current_thread"];

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example main_empty --features hotpath
    #[test]
    fn test_main_empty_params() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "main_empty",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let expected = ["main_empty::example_function", "main_empty::main"];

        let stdout = String::from_utf8_lossy(&output.stdout);

        for expected in expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example main_percentiles --features hotpath
    #[test]
    fn test_main_percentiles_param() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "main_percentiles",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let all_expected = [
            "main_percentiles::example_function",
            "P50",
            "P90",
            "P99",
            "Function",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example main_format --features hotpath
    #[test]
    fn test_main_format_param() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "main_format",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let all_expected = [
            "main_format::example_function",
            "\"hotpath_profiling_mode\"",
            "\"calls\"",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example main_percentiles_format --features hotpath
    #[test]
    fn test_main_percentiles_format_params() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "main_percentiles_format",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let all_expected = [
            "main_percentiles_format::example_function",
            "\"hotpath_profiling_mode\"",
            "\"p75\"",
            "\"p95\"",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
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
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let all_expected = ["N/A*", "only available for tokio current_thread"];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-all-features --example basic_all_features --all-features
    #[test]
    fn test_all_features_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-all-features",
                "--example",
                "basic_all_features",
                "--all-features",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let all_expected = ["i ran"];

        let stdout = String::from_utf8_lossy(&output.stdout);

        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example csv_file_reporter --features hotpath
    #[test]
    fn test_csv_file_reporter_output() {
        use std::fs;
        use std::path::Path;

        let report_path = "hotpath_report.csv";
        if Path::new(report_path).exists() {
            fs::remove_file(report_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "csv_file_reporter",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            Path::new(report_path).exists(),
            "Custom report file was not created"
        );

        let report_content = fs::read_to_string(report_path).expect("Failed to read report file");

        let expected_content = [
            "Function, Calls, Avg, P50, P90, P95, Total, % Total",
            "Functions measured: 4",
            "csv_file_reporter::async_function, 100",
            "csv_file_reporter::sync_function, 100",
            "custom_block, 100",
            "main, 1",
        ];

        for expected in expected_content {
            assert!(
                report_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{report_content}",
            );
        }

        fs::remove_file(report_path).ok();
    }

    // RUST_LOG=info cargo run -p test-tokio-async --example tracing_reporter --features hotpath
    #[test]
    fn test_tracing_reporter_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "tracing_reporter",
                "--features",
                "hotpath",
            ])
            .env("RUST_LOG", "info")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let expected_content = [
            "HotPath Report for: main",
            "Headers: Function, Calls, Avg, P50, P90, P95, Total, % Total",
            "tracing_reporter::async_function, 100",
            "tracing_reporter::sync_function, 100",
            "custom_block, 100",
        ];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\\n{expected}\\n\\nGot:\\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example no_op_block
    #[test]
    fn test_no_op_block_output() {
        let output = Command::new("cargo")
            .args(["run", "-p", "test-tokio-async", "--example", "no_op_block"])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("custom_block output"));
    }

    // cargo run -p test-tokio-async --example custom_guard --features hotpath
    #[test]
    fn test_custom_guard_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "custom_guard",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        let expected_content = [
            "custom_guard::main",
            "custom_guard::sync_function",
            "custom_guard::async_function",
            "custom_block",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example measure_all_mod --features hotpath
    #[test]
    fn test_measure_all_mod_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "measure_all_mod",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_content = [
            "measured_module::sync_function_one",
            "measured_module::async_function_one",
            "measure_all_mod::main",
            "| measured_module::async_function_one | 50    |",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        let not_expected_content = [
            "measured_module::sync_function_two",
            "measured_module::async_function_two",
        ];

        for not_expected in not_expected_content {
            assert!(
                !stdout.contains(not_expected),
                "Not expected:\n{not_expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example measure_all_impl --features hotpath
    #[test]
    fn test_measure_all_impl_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "measure_all_impl",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_content = [
            "measure_all_impl::new",
            "measure_all_impl::add",
            "measure_all_impl::multiply",
            "measure_all_impl::async_increment",
            "measure_all_impl::async_decrement",
            "measure_all_impl::get_value",
            "measure_all_impl::main",
            "| measure_all_impl::add             | 50    |",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
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

        for (features, alloc_self) in test_cases {
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

            if let Some(val) = alloc_self {
                cmd.env("HOTPATH_ALLOC_SELF", val);
            }

            let output = cmd.output().expect("Failed to execute command");

            let env_info = alloc_self
                .map(|v| format!("HOTPATH_ALLOC_SELF={}", v))
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
        use hotpath::json::FunctionsJson;
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
            .env("HOTPATH_METRICS_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        // Test /functions_timing endpoint
        let mut timing_json = String::new();
        let mut last_error = None;

        // Give the server some time to start up
        for _attempt in 0..18 {
            sleep(Duration::from_millis(500));

            match ureq::get("http://localhost:6775/functions_timing").call() {
                Ok(mut response) => {
                    timing_json = response
                        .body_mut()
                        .read_to_string()
                        .expect("Failed to read response body");
                    last_error = None;
                    break;
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }
        }

        if let Some(error) = last_error {
            let _ = child.kill();
            panic!(
                "Failed to connect to /functions_timing after 12 retries: {}",
                error
            );
        }

        // Assert timing JSON contains expected function names
        let timing_expected = [
            "basic::sync_function",
            "basic::async_function",
            "custom_block",
        ];
        for expected in timing_expected {
            assert!(
                timing_json.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{timing_json}",
            );
        }

        // Parse JSON to verify structure
        let timing_response: FunctionsJson =
            serde_json::from_str(&timing_json).expect("Failed to parse timing JSON");

        // Test /functions_alloc endpoint
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

        // Assert alloc JSON contains expected function names
        for expected in timing_expected {
            assert!(
                alloc_json.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{alloc_json}",
            );
        }

        // Parse alloc JSON to verify structure
        let _alloc_response: FunctionsJson =
            serde_json::from_str(&alloc_json).expect("Failed to parse alloc JSON");

        // Test function logs endpoints using first function from timing response
        if let Some(first_function_name) = timing_response.data.0.keys().next() {
            use base64::Engine;
            let encoded_name =
                base64::engine::general_purpose::STANDARD.encode(first_function_name.as_bytes());

            // Test timing logs endpoint
            let timing_logs_url = format!(
                "http://localhost:6775/functions_timing/{}/logs",
                encoded_name
            );
            let timing_logs_response = ureq::get(&timing_logs_url)
                .call()
                .expect("Failed to call /functions_timing/:name/logs endpoint");

            assert_eq!(
                timing_logs_response.status(),
                200,
                "Expected status 200 for /functions_timing/:name/logs endpoint"
            );

            // Test alloc logs endpoint
            let alloc_logs_url = format!(
                "http://localhost:6775/functions_alloc/{}/logs",
                encoded_name
            );
            let alloc_logs_response = ureq::get(&alloc_logs_url)
                .call()
                .expect("Failed to call /functions_alloc/:name/logs endpoint");

            assert_eq!(
                alloc_logs_response.status(),
                200,
                "Expected status 200 for /functions_alloc/:name/logs endpoint"
            );
        }

        let _ = child.kill();
        let _ = child.wait();
    }

    // cargo run -p test-tokio-async --example main_timeout --features hotpath
    #[test]
    fn test_main_timeout_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "main_timeout",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_content = [
            "main_timeout::first_function",
            "main_timeout::second_function",
            "loop_block",
            "main_timeout::main",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // cargo run -p test-tokio-async --example guard_timeout --features hotpath
    #[test]
    fn test_guard_timeout_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "guard_timeout",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_content = [
            "guard_timeout::first_function",
            "guard_timeout::second_function",
            "loop_block",
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // HOTPATH_METRICS_PORT=6776 HOTPATH_METRICS_SERVER_OFF=true TEST_SLEEP_SECONDS=5 cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_disable_http_server() {
        use std::{thread::sleep, time::Duration};

        let mut child = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_METRICS_PORT", "6776")
            .env("HOTPATH_METRICS_SERVER_OFF", "true")
            .env("TEST_SLEEP_SECONDS", "5")
            .spawn()
            .expect("Failed to spawn command");

        sleep(Duration::from_secs(2));

        let result = ureq::get("http://127.0.0.1:6776/functions_timing").call();

        assert!(
            result.is_err(),
            "HTTP request should have failed when HOTPATH_METRICS_SERVER_OFF=true"
        );

        let _ = child.kill();
        let _ = child.wait();
    }
}
