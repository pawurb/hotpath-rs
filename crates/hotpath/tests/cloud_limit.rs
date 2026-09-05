#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::process::Command;

    use hotpath::json::{JsonFunctionsCpu, JsonReport};

    fn run_example(
        package: &str,
        example: &str,
        features: &str,
        upload: bool,
        env: &[(&str, &str)],
    ) -> JsonReport {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            package,
            "--example",
            example,
            "--features",
            features,
        ])
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
        .env_remove("HOTPATH_LIMIT")
        .env_remove("HOTPATH_FUNCTIONS_LIMIT")
        .env_remove("HOTPATH_UPLOAD_LIMIT");
        if upload {
            cmd.env("HOTPATH_UPLOAD", "1");
        } else {
            cmd.env_remove("HOTPATH_UPLOAD");
        }
        for (key, value) in env {
            cmd.env(key, value);
        }
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json_start = stdout.find('{').expect("No JSON report in output");
        serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report")
    }

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-alloc,hotpath-cloud
    fn run_all_features(upload: bool, env: &[(&str, &str)]) -> JsonReport {
        run_example(
            "test-all-features",
            "basic_all_features",
            "hotpath,hotpath-alloc,hotpath-cloud",
            upload,
            env,
        )
    }

    // cargo run -p test-axum --example route_scope --features hotpath,hotpath-cloud
    fn run_route_scope(upload: bool, env: &[(&str, &str)]) -> JsonReport {
        run_example(
            "test-axum",
            "route_scope",
            "hotpath,hotpath-cloud",
            upload,
            env,
        )
    }

    struct Counts {
        section: &'static str,
        total: usize,
        included: usize,
        len: usize,
    }

    /// Every limited section present in the report, in schema order.
    fn counts(report: &JsonReport) -> Vec<Counts> {
        let mut out = Vec::new();
        let mut push = |section, total, included, len| {
            out.push(Counts {
                section,
                total,
                included,
                len,
            })
        };
        if let Some(l) = &report.functions_timing {
            push(
                "functions_timing",
                l.total_count,
                l.included_count,
                l.data.len(),
            );
        }
        if let Some(l) = &report.functions_alloc {
            push(
                "functions_alloc",
                l.total_count,
                l.included_count,
                l.data.len(),
            );
        }
        if let Some(JsonFunctionsCpu::Ok(l)) = &report.functions_cpu {
            push(
                "functions_cpu",
                l.total_count,
                l.included_count,
                l.data.len(),
            );
        }
        if let Some(l) = &report.channels {
            push("channels", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.streams {
            push("streams", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.futures {
            push("futures", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.rw_locks {
            push("rw_locks", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.mutexes {
            push("mutexes", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.sql {
            push("sql", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.http {
            push("http", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.server {
            push("server", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.io {
            push("io", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.threads {
            push("threads", l.total_count, l.included_count, l.data.len());
        }
        if let Some(l) = &report.debug {
            push("debug", l.total_count, l.included_count, l.entries.len());
        }
        assert!(!out.is_empty(), "report has no limited sections");
        out
    }

    fn assert_limited(report: &JsonReport, limit: usize, expected: &[&str]) {
        let counts = counts(report);
        for name in expected {
            assert!(
                counts.iter().any(|c| c.section == *name),
                "{name} section missing from report"
            );
        }
        for c in counts {
            assert_eq!(c.included, c.len, "{}: included_count", c.section);
            assert!(
                c.total >= c.included,
                "{}: total_count {} < included_count {}",
                c.section,
                c.total,
                c.included
            );
            let expected = if limit > 0 {
                c.total.min(limit)
            } else {
                c.total
            };
            assert_eq!(c.included, expected, "{}: included_count", c.section);
        }
    }

    #[test]
    fn display_limit_truncates_every_section() {
        let report = run_all_features(false, &[("HOTPATH_LIMIT", "1")]);
        assert_limited(
            &report,
            1,
            &[
                "functions_timing",
                "functions_alloc",
                "channels",
                "mutexes",
                "rw_locks",
                "threads",
            ],
        );
        let functions = report.functions_timing.unwrap();
        assert!(
            functions.total_count > 1,
            "example must measure several functions"
        );

        let report = run_route_scope(false, &[("HOTPATH_LIMIT", "1")]);
        assert_limited(&report, 1, &["sql", "http", "server"]);
        assert!(report.sql.unwrap().total_count > 1);
    }

    #[test]
    fn upload_ignores_display_limits() {
        let report = run_all_features(
            true,
            &[("HOTPATH_LIMIT", "1"), ("HOTPATH_FUNCTIONS_LIMIT", "1")],
        );
        assert_limited(
            &report,
            0,
            &["functions_timing", "functions_alloc", "threads"],
        );
        assert!(report.functions_timing.unwrap().included_count > 1);

        let report = run_route_scope(true, &[("HOTPATH_LIMIT", "1")]);
        assert_limited(&report, 0, &["sql", "http", "server"]);
        assert!(report.sql.unwrap().included_count > 1);
    }

    #[test]
    fn upload_limit_caps_every_section() {
        let report = run_all_features(true, &[("HOTPATH_UPLOAD_LIMIT", "2")]);
        assert_limited(
            &report,
            2,
            &["functions_timing", "functions_alloc", "threads"],
        );
        assert_eq!(report.functions_timing.unwrap().included_count, 2);

        let report = run_route_scope(true, &[("HOTPATH_UPLOAD_LIMIT", "2")]);
        assert_limited(&report, 2, &["sql", "http", "server"]);
        assert_eq!(report.sql.unwrap().included_count, 2);
    }
}
