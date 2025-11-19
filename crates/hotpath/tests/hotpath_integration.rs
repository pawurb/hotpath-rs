#[cfg(test)]
pub mod tests {
    use std::process::Command;

    #[test]
    fn test_basic_output() {
        let features = ["", "hotpath-alloc-bytes-total", "hotpath-alloc-count-total"];

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

    #[test]
    fn test_early_returns_output() {
        let features = [
            "hotpath",
            "hotpath-alloc-bytes-total",
            "hotpath-alloc-count-total",
        ];
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
                "hotpath,hotpath-alloc-bytes-total",
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
                "hotpath,hotpath-alloc-bytes-total",
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

    #[test]
    fn test_json_file_reporter_output() {
        use std::fs;
        use std::path::Path;

        let report_path = "hotpath_report.json";
        if Path::new(report_path).exists() {
            fs::remove_file(report_path).ok();
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-tokio-async",
                "--example",
                "json_file_reporter",
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Report saved to hotpath_report.json"),
            "Expected success message not found in stdout: {stdout}"
        );

        assert!(
            Path::new(report_path).exists(),
            "JSON report file was not created"
        );

        let report_content = fs::read_to_string(report_path).expect("Failed to read report file");

        let expected_content = [
            "\"hotpath_profiling_mode\"",
            "\"timing\"",
            "\"total_elapsed\"",
            "\"caller_name\"",
            "\"main\"",
            "\"output\"",
            "\"json_file_reporter::async_function\"",
            "\"json_file_reporter::sync_function\"",
            "\"custom_block\"",
            "\"calls\"",
            "\"avg\"",
            "\"total\"",
            "\"percent_total\"",
        ];

        for expected in expected_content {
            assert!(
                report_content.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{report_content}",
            );
        }

        serde_json::from_str::<serde_json::Value>(&report_content)
            .expect("Report content is not valid JSON");

        fs::remove_file(report_path).ok();
    }

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
        ];

        let stdout = String::from_utf8_lossy(&output.stdout);
        for expected in expected_content {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

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
                "hotpath,hotpath-alloc-bytes-total",
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

    #[test]
    fn test_multithread_alloc_no_panic() {
        let test_cases = [
            ("hotpath,hotpath-alloc-count-total", None),
            ("hotpath,hotpath-alloc-bytes-total", None),
            ("hotpath,hotpath-alloc-count-total", Some("true")),
            ("hotpath,hotpath-alloc-bytes-total", Some("true")),
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
}
