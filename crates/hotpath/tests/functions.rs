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
            "\"profiling_mode\"",
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
            "\"profiling_mode\"",
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("basic_smol::main"),
            "Expected basic_smol::main in output\n\nGot:\n{stdout}",
        );
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

    // cargo check -p test-tokio-async --example measure_all_impl_return_closure --features hotpath
    #[test]
    fn test_measure_all_impl_return_closure_compiles() {
        let output = Command::new("cargo")
            .args([
                "check",
                "-p",
                "test-tokio-async",
                "--example",
                "measure_all_impl_return_closure",
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
            .env("HOTPATH_METRICS_PORT", "6775")
            .env("TEST_SLEEP_SECONDS", "10")
            .spawn()
            .expect("Failed to spawn command");

        // Test /functions_timing endpoint
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

        // Parse JSON to verify structure
        let timing_response: JsonFunctionsList =
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
        let _alloc_response: JsonFunctionsList =
            serde_json::from_str(&alloc_json).expect("Failed to parse alloc JSON");

        if let Some(first) = timing_response.data.first() {
            let function_id = first.id;

            // Test timing logs endpoint
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

            // Test alloc logs endpoint
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
            .env("HOTPATH_SHUTDOWN_MS", "1000")
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

    // cargo run -p test-tokio-async --example guard_timeout_functions --features hotpath
    #[test]
    fn test_guard_timeout_functions_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "guard_timeout_functions",
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

        let expected_content = ["guard_timeout_functions::looping_function"];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    // HOTPATH_EXCLUDE_WRAPPER=1 cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_exclude_wrapper_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_EXCLUDE_WRAPPER", "1")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let expected_content = [
            "basic::sync_function",
            "basic::async_function",
            "custom_block",
        ];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        assert!(
            !stdout.contains("\"name\":\"basic::main\""),
            "Wrapper function 'basic::main' should not be in data array when HOTPATH_EXCLUDE_WRAPPER=1\n\nGot:\n{stdout}"
        );
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

    // cargo run -p test-tokio-async --example functions_file_output --features hotpath
    #[test]
    fn test_functions_file_output() {
        use std::fs;
        use std::path::Path;

        let output_path = "tmp/functions_output_test.json";

        fs::create_dir_all("tmp").ok();
        if Path::new(output_path).exists() {
            fs::remove_file(output_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "functions_file_output",
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
            Path::new(output_path).exists(),
            "Output file was not created at {}",
            output_path
        );

        let file_content = fs::read_to_string(output_path).expect("Failed to read output file");

        let expected_content = [
            "functions_file_output::example_function",
            "\"profiling_mode\"",
            "\"calls\"",
        ];

        for expected in expected_content {
            assert!(
                file_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{file_content}",
            );
        }

        fs::remove_file(output_path).ok();
    }

    // HOTPATH_OUTPUT_FORMAT=none cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_format_none_suppresses_output() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "none")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            stdout.contains("custom_block output"),
            "Application output should still be present.\nGot:\n{stdout}"
        );

        let not_expected = [
            "basic::sync_function",
            "basic::async_function",
            "profiling_mode",
            "percent_total",
        ];

        for not_exp in not_expected {
            assert!(
                !stdout.contains(not_exp),
                "Profiling output should be suppressed with HOTPATH_OUTPUT_FORMAT=none.\nFound: {not_exp}\nGot:\n{stdout}"
            );
        }
    }

    // HOTPATH_OUTPUT_PATH=tmp/env_override.json cargo run -p test-tokio-async --example functions_file_output --features hotpath
    #[test]
    fn test_hotpath_output_path_env_override() {
        use std::fs;
        use std::path::Path;

        let programmatic_path = "tmp/functions_output_test.json";
        let env_override_path = "tmp/env_override.json";

        fs::create_dir_all("tmp").ok();
        if Path::new(programmatic_path).exists() {
            fs::remove_file(programmatic_path).ok();
        }
        if Path::new(env_override_path).exists() {
            fs::remove_file(env_override_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "functions_file_output",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_PATH", env_override_path)
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            Path::new(env_override_path).exists(),
            "Output file was not created at env override path {}",
            env_override_path
        );

        assert!(
            !Path::new(programmatic_path).exists(),
            "Output file should NOT be created at programmatic path {} when HOTPATH_OUTPUT_PATH is set",
            programmatic_path
        );

        fs::remove_file(env_override_path).ok();
    }

    // HOTPATH_OUTPUT_FORMAT=table HOTPATH_FOCUS=basic cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_focus_substring_filter() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "table")
            .env("HOTPATH_FOCUS", "basic")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        let expected_content = [
            "basic::sync_function",
            "basic::async_function",
            "basic::main",
        ];

        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        assert!(
            !stdout.contains("| custom_block"),
            "custom_block should be filtered out by HOTPATH_FOCUS=basic\n\nGot:\n{stdout}"
        );
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

    // HOTPATH_OUTPUT_FORMAT=table HOTPATH_FOCUS='/(custom)/' cargo run -p test-tokio-async --example basic --features hotpath
    #[test]
    fn test_focus_regex_filter() {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "basic",
                "--features",
                "hotpath",
            ])
            .env("HOTPATH_OUTPUT_FORMAT", "table")
            .env("HOTPATH_FOCUS", "/(custom)/")
            .output()
            .expect("Failed to execute command");

        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            stdout.contains("| custom_block"),
            "Expected custom_block in profiling output\n\nGot:\n{stdout}",
        );

        assert!(
            stdout.contains("basic::main"),
            "Wrapper function basic::main should never be excluded by HOTPATH_FOCUS\n\nGot:\n{stdout}",
        );

        let not_expected = ["| basic::sync_function", "| basic::async_function"];

        for not_exp in not_expected {
            assert!(
                !stdout.contains(not_exp),
                "{not_exp} should be filtered out by HOTPATH_FOCUS=/(custom)/\n\nGot:\n{stdout}"
            );
        }
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
