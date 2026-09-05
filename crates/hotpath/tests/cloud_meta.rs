#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use hotpath::json::{JsonMeta, JsonReport};

    const CI_SHA: &str = "2222222222222222222222222222222222222222";
    const BASE_SHA: &str = "3333333333333333333333333333333333333333";

    /// cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-cloud
    fn run_example(envs: &[(&str, &str)]) -> JsonMeta {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "basic_all_features",
            "--features",
            "hotpath,hotpath-cloud",
        ])
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env("HOTPATH_REPORT", "functions-timing")
        .env_remove("HOTPATH_UPLOAD")
        .env_remove("HOTPATH_BENCHMARK");
        for var in [
            "GITHUB_ACTIONS",
            "GITHUB_SHA",
            "GITHUB_REF",
            "GITHUB_REPOSITORY",
            "GITHUB_REPOSITORY_ID",
            "GITHUB_EVENT_NAME",
            "GITHUB_EVENT_PATH",
            "GITHUB_RUN_ID",
            "GITHUB_WORKFLOW",
            "GITHUB_ACTOR",
        ] {
            cmd.env_remove(var);
        }
        cmd.envs(envs.iter().copied());

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
            .meta
    }

    fn write_event_payload() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "hotpath-cloud-meta-event-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            format!(
                r#"{{"action":"synchronize","pull_request":{{"number":42,"base":{{"ref":"main","sha":"{BASE_SHA}"}}}}}}"#
            ),
        )
        .expect("event payload written");
        path
    }

    #[test]
    fn meta_outside_ci_describes_the_checkout() {
        let meta = run_example(&[]);

        assert!(
            meta.ci.is_none(),
            "no CI provider outside CI: {:?}",
            meta.ci
        );
        assert_eq!(meta.benchmark.as_deref(), Some("default"));

        let git = meta.git.expect("git info from the checkout");
        assert_eq!(git.sha.len(), 40, "sha: {}", git.sha);
        assert_eq!(git.repository.as_deref(), Some("pawurb/hotpath-rs"));
        // A merge base needs a commit-graph walk the `.git` reader avoids.
        assert_eq!(git.base_sha, None);
    }

    #[test]
    fn meta_in_github_actions_describes_the_run() {
        let event_path = write_event_payload();
        let meta = run_example(&[
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_SHA", CI_SHA),
            ("GITHUB_REF", "refs/pull/42/merge"),
            ("GITHUB_REPOSITORY", "pawurb/hotpath-rs"),
            ("GITHUB_REPOSITORY_ID", "123456"),
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_EVENT_PATH", &event_path.to_string_lossy()),
            ("GITHUB_RUN_ID", "987654321"),
            ("GITHUB_WORKFLOW", "benchmarks"),
            ("GITHUB_ACTOR", "pawurb"),
            ("HOTPATH_BENCHMARK", "timing-linux"),
        ]);
        let _ = std::fs::remove_file(&event_path);

        let git = meta.git.expect("git info from the environment");
        assert_eq!(git.sha, CI_SHA);
        assert_eq!(git.r#ref.as_deref(), Some("refs/pull/42/merge"));
        assert_eq!(git.base_sha.as_deref(), Some(BASE_SHA));
        assert_eq!(git.repository.as_deref(), Some("pawurb/hotpath-rs"));

        let ci = meta.ci.expect("ci info");
        assert_eq!(ci.provider, "github-actions");
        assert_eq!(ci.event.as_deref(), Some("pull_request"));
        assert_eq!(ci.pr_number, Some(42));
        assert_eq!(ci.run_id.as_deref(), Some("987654321"));
        assert_eq!(ci.workflow.as_deref(), Some("benchmarks"));
        assert_eq!(ci.actor.as_deref(), Some("pawurb"));
        assert_eq!(ci.repository_id.as_deref(), Some("123456"));

        assert_eq!(meta.benchmark.as_deref(), Some("timing-linux"));
    }

    #[test]
    fn push_runs_carry_no_base_sha() {
        let meta = run_example(&[
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_SHA", CI_SHA),
            ("GITHUB_REF", "refs/heads/main"),
            ("GITHUB_EVENT_NAME", "push"),
            ("GITHUB_EVENT_PATH", "/nonexistent/event.json"),
        ]);

        let git = meta.git.expect("git info from the environment");
        assert_eq!(git.sha, CI_SHA);
        assert_eq!(git.r#ref.as_deref(), Some("refs/heads/main"));
        assert_eq!(git.base_sha, None);

        let ci = meta.ci.expect("ci info");
        assert_eq!(ci.event.as_deref(), Some("push"));
        assert_eq!(ci.pr_number, None);
    }
}
