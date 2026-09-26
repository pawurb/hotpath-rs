#[cfg(all(test, feature = "cloud"))]
mod tests {
    //! `hotpath cloud auth|repos|benchmarks|report` against a mock
    //! hotpath.rs: the bearer request each sends, the JSON it re-emits, the
    //! error JSON on stderr (server bodies verbatim, client failures as
    //! `{"error": ...}`) with exit 1, argument validation before any request
    //! (and before the token is read), clap usage errors with exit 2, and
    //! that the token never reaches stdout or stderr.
    //!
    //! cargo test -p hotpath --features cloud --test cloud_cli

    use std::process::{Command, Output};

    use hotpath::json::cloud_api::{
        ApiError, ApiErrorCode, AuthStatus, RepoList, Report, ReportSummary, TokenStatus,
    };
    use mockito::{Matcher, Server, ServerGuard};
    use time::macros::datetime;

    const TOKEN: &str = "hpat_5f3c9a1b2d4e6f7a8b9c0d1e2f3a4b5c";
    const AUTH_BODY: &str =
        r#"{"login":"pawurb","token":{"name":"laptop","expires_at":"2027-01-01T00:00:00Z"}}"#;
    const REPOS_BODY: &str = r#"{"repositories":[{"full_name":"pawurb/hotpath-rs","private":false,"visibility_public":true,"benchmarks":[{"name":"ci","reports":412,"latest_report_at":"2026-09-25T18:03:11Z"},{"name":"empty","reports":0,"latest_report_at":null}]},{"full_name":"pawurb/private-thing","private":true,"visibility_public":false,"benchmarks":[]}]}"#;
    const BENCHMARKS_BODY: &str = r#"{"repository":"pawurb/hotpath-rs","benchmarks":[{"name":"ci","reports":412,"latest_report_at":"2026-09-25T18:03:11Z"}]}"#;
    const BENCHMARKS_PATH: &str = "/api/v1/repos/pawurb/hotpath-rs/benchmarks";
    const REPORTS_PATH: &str = "/api/v1/repos/pawurb/hotpath-rs/benchmarks/ci/reports";
    const REPORT_ID: &str = "0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a";
    const SHA: &str = "9ab2000000000000000000000000000000000000";
    const SUMMARY_BODY: &str = r#"{"id":"0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a","repository":"pawurb/hotpath-rs","benchmark":"ci","event":"pull_request","commit_sha":"3f1c000000000000000000000000000000000000","head_sha":"9ab2000000000000000000000000000000000000","base_sha":"77de000000000000000000000000000000000000","git_ref":null,"base_ref":"main","head_ref":"channel-delay","pr_number":105,"run_id":"18237461234","workflow":"CI","actor":"pawurb","ci_provider":"github-actions","hotpath_version":"0.26.1","user_metadata":{"profile":"release"},"baseline_id":"0199a3b0-0000-7000-8000-000000000000","comment_url":"https://github.com/pawurb/hotpath-rs/pull/105#issuecomment-1","size_bytes":81234,"created_at":"2026-09-25T18:03:11Z","dashboard_url":"https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a"}"#;

    /// `SUMMARY_BODY` plus a payload in the writer's (unsorted) key order.
    fn report_body() -> String {
        format!(
            "{},\"payload\":{{\"version\":\"0.26.1\",\"meta\":{{}}}}}}",
            &SUMMARY_BODY[..SUMMARY_BODY.len() - 1]
        )
    }

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text.trim()).unwrap_or_else(|e| panic!("not JSON: {e}\n{text}"))
    }

    /// A 200 mock of a report route with this exact query.
    fn mock_report(server: &mut ServerGuard, path: &str, query: &str, body: &str) -> mockito::Mock {
        server
            .mock("GET", path)
            .match_query(Matcher::Exact(query.into()))
            .match_header("authorization", format!("Bearer {TOKEN}").as_str())
            .with_status(200)
            .with_header("content-type", "application/json; charset=utf-8")
            .with_body(body)
            .create()
    }

    fn report_args<'a>(selector: &[&'a str]) -> Vec<&'a str> {
        let mut args = vec!["report", "--repo", "pawurb/hotpath-rs", "--benchmark", "ci"];
        args.extend_from_slice(selector);
        args
    }

    fn hotpath(server: &ServerGuard, token: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hotpath"));
        cmd.arg("cloud")
            .args(args)
            .env("HOTPATH_API_URL", format!("{}/", server.url()))
            .env_remove("HOTPATH_API_TOKEN");
        if let Some(token) = token {
            cmd.env("HOTPATH_API_TOKEN", token);
        }
        let output = cmd.output().expect("failed to run the hotpath binary");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stdout.contains(TOKEN) && !stderr.contains(TOKEN),
            "the token leaked into the output:\nstdout: {stdout}\nstderr: {stderr}"
        );
        output
    }

    fn stdout(output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn stderr(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    /// The `error` of a client-built failure on stderr, which is always one
    /// JSON object with that single key.
    fn client_error(output: &Output) -> String {
        let stderr = stderr(output);
        let value: serde_json::Value = serde_json::from_str(&stderr).unwrap_or_else(|e| {
            panic!("stderr is not JSON: {e}\n{stderr}");
        });
        let object = value.as_object().expect("stderr is not a JSON object");
        assert_eq!(object.len(), 1, "{stderr}");
        object["error"]
            .as_str()
            .expect("error is not a string")
            .to_string()
    }

    fn error_body(code: ApiErrorCode, error: &str) -> String {
        serde_json::to_string(&ApiError {
            error: error.into(),
            code,
        })
        .unwrap()
    }

    fn mock_get(server: &mut ServerGuard, path: &str, body: &str) -> mockito::Mock {
        server
            .mock("GET", path)
            .match_header("authorization", format!("Bearer {TOKEN}").as_str())
            .match_header("user-agent", Matcher::Regex("^hotpath-cli/[0-9]".into()))
            .with_status(200)
            .with_header("content-type", "application/json; charset=utf-8")
            .with_body(body)
            .create()
    }

    fn mock_auth(server: &mut ServerGuard) -> mockito::Mock {
        mock_get(server, "/api/v1/auth", AUTH_BODY)
    }

    #[test]
    fn auth_prints_the_status_body_compact_by_default() {
        let mut server = Server::new();
        let mock = mock_auth(&mut server);

        let output = hotpath(&server, Some(TOKEN), &["auth"]);
        mock.assert();
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(stdout(&output), format!("{AUTH_BODY}\n"));
        assert_eq!(stderr(&output), "");

        let status: AuthStatus = serde_json::from_str(stdout(&output).trim()).unwrap();
        assert_eq!(
            status,
            AuthStatus {
                login: "pawurb".into(),
                token: TokenStatus {
                    name: "laptop".into(),
                    expires_at: datetime!(2027-01-01 00:00:00 UTC),
                },
            }
        );
    }

    #[test]
    fn auth_pretty_and_output_file() {
        let mut server = Server::new();
        let mock = mock_auth(&mut server).expect(2);

        let output = hotpath(&server, Some(TOKEN), &["auth", "--pretty"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        let expected: AuthStatus = serde_json::from_str(AUTH_BODY).unwrap();
        assert_eq!(
            stdout(&output),
            format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
        );

        let path = std::env::temp_dir().join("hotpath_cloud_cli_auth_output.json");
        let _ = std::fs::remove_file(&path);
        let output = hotpath(
            &server,
            Some(TOKEN),
            &["auth", "--output", path.to_str().unwrap()],
        );
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(stdout(&output), "");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{AUTH_BODY}\n")
        );
        let _ = std::fs::remove_file(&path);

        mock.assert();
    }

    #[test]
    fn auth_without_a_token_does_not_call_the_server() {
        let mut server = Server::new();
        let mock = server.mock("GET", "/api/v1/auth").expect(0).create();

        for token in [None, Some(""), Some("   ")] {
            let output = hotpath(&server, token, &["auth"]);
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(stdout(&output), "");
            assert_eq!(
                client_error(&output),
                "HOTPATH_API_TOKEN is not set. Create a token at https://hotpath.rs/app/tokens and export it."
            );
        }
        mock.assert();
    }

    #[test]
    fn auth_rejected_token_prints_the_server_body_verbatim() {
        let mut server = Server::new();
        // Key order the client would not produce itself, so a byte-for-byte
        // match proves the body is not re-serialized.
        let body = r#"{"error":"The token is unknown, expired or revoked. Create one at /app/tokens.","code":"invalid_token"}"#;
        let mock = server
            .mock("GET", "/api/v1/auth")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_header("www-authenticate", "Bearer")
            .with_header("x-request-id", "1bac4db9-15a")
            .with_body(body)
            .create();

        let output = hotpath(&server, Some(TOKEN), &["auth"]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        assert_eq!(stderr(&output), format!("{body}\n"));
    }

    #[test]
    fn auth_server_error_passes_through_without_client_hints() {
        // Status, body: the codes the client used to append advice to, plus
        // one it does not know. Every body prints as sent, and nothing more.
        let cases: [(u16, String); 4] = [
            (
                401,
                error_body(
                    ApiErrorCode::GithubAuthorizationExpired,
                    "Your GitHub authorization expired.",
                ),
            ),
            (
                429,
                error_body(ApiErrorCode::RateLimited, "Too many requests."),
            ),
            (500, error_body(ApiErrorCode::Internal, "Something broke.")),
            (
                401,
                r#"{"error":"A code this client does not know.","code":"quota_exceeded"}"#.into(),
            ),
        ];
        for (status, body) in cases {
            let mut server = Server::new();
            let mock = server
                .mock("GET", "/api/v1/auth")
                .with_status(status.into())
                .with_header("content-type", "application/json")
                .with_header("retry-after", "30")
                .with_header("x-request-id", "abc")
                .with_body(&body)
                .create();

            let output = hotpath(&server, Some(TOKEN), &["auth"]);
            mock.assert();
            assert_eq!(output.status.code(), Some(1), "{body}");
            assert_eq!(stdout(&output), "", "{body}");
            assert_eq!(stderr(&output), format!("{body}\n"));
        }
    }

    #[test]
    fn auth_pretty_indents_the_server_error() {
        let mut server = Server::new();
        let body = error_body(ApiErrorCode::InvalidToken, "Nope.");
        let mock = server
            .mock("GET", "/api/v1/auth")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(&body)
            .create();

        let output = hotpath(&server, Some(TOKEN), &["auth", "--pretty"]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let expected: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            stderr(&output),
            format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
        );
    }

    #[test]
    fn auth_non_json_error_quotes_status_and_body_prefix() {
        let mut server = Server::new();
        let page = format!("<html>{}</html>", "x".repeat(400));
        let mock = server
            .mock("GET", "/api/v1/auth")
            .with_status(502)
            .with_header("content-type", "text/html")
            .with_body(&page)
            .create();

        let output = hotpath(&server, Some(TOKEN), &["auth"]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        assert_eq!(
            client_error(&output),
            format!("HTTP 502: {}...", &page[..200])
        );
    }

    #[test]
    fn auth_unreadable_success_body_is_an_error() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/api/v1/auth")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"login":"pawurb"}"#)
            .create();

        let output = hotpath(&server, Some(TOKEN), &["auth"]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let error = client_error(&output);
        assert!(error.starts_with("invalid response from "), "{error}");
    }

    #[test]
    fn auth_unreachable_server_is_exit_1() {
        // A port nothing listens on: bind, read it back, release it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        drop(listener);

        let output = Command::new(env!("CARGO_BIN_EXE_hotpath"))
            .args(["cloud", "auth"])
            .env("HOTPATH_API_URL", &url)
            .env("HOTPATH_API_TOKEN", TOKEN)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let error = client_error(&output);
        assert!(
            error.starts_with(&format!("request to {url} failed: ")),
            "{error}"
        );
        assert!(!error.contains(TOKEN), "{error}");
    }

    #[test]
    fn repos_prints_the_list_compact_and_pretty() {
        let mut server = Server::new();
        let mock = mock_get(&mut server, "/api/v1/repos", REPOS_BODY).expect(2);

        let output = hotpath(&server, Some(TOKEN), &["repos"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(stdout(&output), format!("{REPOS_BODY}\n"));
        assert_eq!(stderr(&output), "");

        let output = hotpath(&server, Some(TOKEN), &["repos", "--pretty"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        let expected: RepoList = serde_json::from_str(REPOS_BODY).unwrap();
        assert_eq!(
            stdout(&output),
            format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
        );

        mock.assert();
    }

    #[test]
    fn benchmarks_requests_the_repo_flag_repository() {
        let mut server = Server::new();
        let mock = mock_get(&mut server, BENCHMARKS_PATH, BENCHMARKS_BODY);

        let output = hotpath(
            &server,
            Some(TOKEN),
            &["benchmarks", "--repo", "pawurb/hotpath-rs"],
        );
        mock.assert();
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(stdout(&output), format!("{BENCHMARKS_BODY}\n"));
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn benchmarks_rejects_a_missing_or_bad_repo_flag_without_a_request() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", Matcher::Regex("^/api/v1/repos/".into()))
            .expect(0)
            .create();

        let output = hotpath(&server, Some(TOKEN), &["benchmarks"]);
        assert_eq!(output.status.code(), Some(2), "clap usage error");
        assert_eq!(stdout(&output), "");
        assert!(stderr(&output).contains("--repo"), "{}", stderr(&output));

        for value in [
            "nope", "a/b/c", "a/..", "./b", "a/", "/b", "a b/c", "a/b?x=1",
        ] {
            // No token either: the bad argument is what gets reported.
            for token in [Some(TOKEN), None] {
                let output = hotpath(&server, token, &["benchmarks", "--repo", value]);
                assert_eq!(output.status.code(), Some(1), "{value}");
                assert_eq!(stdout(&output), "", "{value}");
                let error = client_error(&output);
                assert!(error.contains("invalid --repo"), "{value}: {error}");
                assert!(error.contains(value), "{value}: {error}");
            }
        }
        mock.assert();
    }

    #[test]
    fn benchmarks_errors_print_the_server_body_verbatim() {
        // The two codes `auth` never produces: an unknown, invisible or
        // App-less repository, and a lapsed GitHub authorization.
        let cases: [(u16, String); 2] = [
            (
                404,
                error_body(
                    ApiErrorCode::NotFound,
                    "Repository pawurb/hotpath-rs not found.",
                ),
            ),
            (
                401,
                error_body(
                    ApiErrorCode::GithubAuthorizationExpired,
                    "Your GitHub authorization expired. Log in at https://hotpath.rs/app once.",
                ),
            ),
        ];
        for (status, body) in cases {
            let mut server = Server::new();
            let mock = server
                .mock("GET", BENCHMARKS_PATH)
                .with_status(status.into())
                .with_header("content-type", "application/json")
                .with_header("x-request-id", "req-1")
                .with_body(&body)
                .create();

            let output = hotpath(
                &server,
                Some(TOKEN),
                &["benchmarks", "--repo", "pawurb/hotpath-rs"],
            );
            mock.assert();
            assert_eq!(output.status.code(), Some(1), "{body}");
            assert_eq!(stdout(&output), "", "{body}");
            assert_eq!(stderr(&output), format!("{body}\n"));
        }
    }

    #[test]
    fn report_by_pr_prints_the_report_compact_and_pretty() {
        let mut server = Server::new();
        let body = report_body();
        let mock = mock_report(
            &mut server,
            &format!("{REPORTS_PATH}/latest"),
            "pr=105",
            &body,
        )
        .expect(2);

        let output = hotpath(&server, Some(TOKEN), &report_args(&["--pr", "105"]));
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(stderr(&output), "");
        let printed = stdout(&output);
        assert!(!printed.trim_end().contains('\n'), "compact: {printed}");
        assert_eq!(json(&printed), json(&body));
        // Nullable fields print as `null`, the payload comes last.
        assert!(printed.contains(r#""git_ref":null"#), "{printed}");
        assert!(
            printed.ends_with(",\"payload\":{\"meta\":{},\"version\":\"0.26.1\"}}\n"),
            "{printed}"
        );

        let output = hotpath(
            &server,
            Some(TOKEN),
            &report_args(&["--pr", "105", "--pretty"]),
        );
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        let expected: Report = serde_json::from_str(&body).unwrap();
        assert_eq!(
            stdout(&output),
            format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
        );

        mock.assert();
    }

    #[test]
    fn report_by_commit_sends_the_event_and_lowercases_the_sha() {
        let mut server = Server::new();
        let body = report_body();
        let mock = mock_report(
            &mut server,
            &format!("{REPORTS_PATH}/latest"),
            &format!("commit={SHA}&event=push"),
            &body,
        )
        .expect(2);

        for sha in [SHA.to_string(), SHA.to_ascii_uppercase()] {
            let output = hotpath(
                &server,
                Some(TOKEN),
                &report_args(&["--commit", &sha, "--event", "push"]),
            );
            assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
            assert_eq!(json(&stdout(&output)), json(&body));
        }
        mock.assert();
    }

    #[test]
    fn report_by_id_without_payload_prints_the_summary() {
        let mut server = Server::new();
        let mock = mock_report(
            &mut server,
            &format!("{REPORTS_PATH}/{REPORT_ID}"),
            "payload=false",
            SUMMARY_BODY,
        );

        let output = hotpath(
            &server,
            Some(TOKEN),
            &report_args(&["--id", REPORT_ID, "--no-payload"]),
        );
        mock.assert();
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(stdout(&output), format!("{SUMMARY_BODY}\n"));
        let summary: ReportSummary = serde_json::from_str(stdout(&output).trim()).unwrap();
        assert_eq!(summary.id, REPORT_ID);
    }

    #[test]
    fn report_not_uploaded_yet_is_the_server_404_verbatim() {
        let mut server = Server::new();
        let body = error_body(
            ApiErrorCode::NotFound,
            "No report of benchmark ci matches that commit.",
        );
        let mock = server
            .mock("GET", format!("{REPORTS_PATH}/latest").as_str())
            .match_query(Matcher::Exact(format!("commit={SHA}")))
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(&body)
            .create();

        let output = hotpath(&server, Some(TOKEN), &report_args(&["--commit", SHA]));
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        assert_eq!(stderr(&output), format!("{body}\n"));
    }

    #[test]
    fn report_rejects_bad_values_before_the_token_or_a_request() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", Matcher::Regex("^/api/v1/".into()))
            .expect(0)
            .create();

        let long_name = "a".repeat(65);
        let non_hex = "g".repeat(40);
        let cases: [(Vec<&str>, &str); 9] = [
            (
                report_args(&["--commit", "9ab2"]),
                "invalid --commit `9ab2`",
            ),
            (report_args(&["--commit", &non_hex]), "invalid --commit"),
            (report_args(&["--pr", "0"]), "invalid --pr `0`"),
            (
                report_args(&["--id", "0199a3c2/../../x"]),
                "invalid --id `0199a3c2/../../x`",
            ),
            (
                vec![
                    "report",
                    "--repo",
                    "pawurb/hotpath-rs",
                    "--benchmark",
                    "a/b",
                    "--pr",
                    "1",
                ],
                "invalid --benchmark `a/b`",
            ),
            (
                vec![
                    "report",
                    "--repo",
                    "pawurb/hotpath-rs",
                    "--benchmark",
                    "..",
                    "--pr",
                    "1",
                ],
                "invalid --benchmark `..`",
            ),
            (
                vec![
                    "report",
                    "--repo",
                    "pawurb/hotpath-rs",
                    "--benchmark",
                    &long_name,
                    "--pr",
                    "1",
                ],
                "invalid --benchmark",
            ),
            (
                vec!["report", "--repo", "nope", "--benchmark", "ci", "--pr", "1"],
                "invalid --repo `nope`",
            ),
            (
                vec!["report", "--repo", "a/..", "--benchmark", "ci", "--pr", "1"],
                "invalid --repo `a/..`",
            ),
        ];
        for (args, expected) in cases {
            let output = hotpath(&server, None, &args);
            assert_eq!(output.status.code(), Some(1), "{args:?}");
            assert_eq!(stdout(&output), "", "{args:?}");
            let error = client_error(&output);
            assert!(error.starts_with(expected), "{args:?}: {error}");
        }
        assert!(
            client_error(&hotpath(&server, None, &report_args(&["--commit", "9ab2"])))
                .contains("git rev-parse HEAD")
        );
        mock.assert();
    }

    #[test]
    fn report_selector_misuse_is_a_clap_usage_error() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", Matcher::Regex("^/api/v1/".into()))
            .expect(0)
            .create();

        for selector in [
            vec![],
            vec!["--pr", "1", "--commit", SHA],
            vec!["--pr", "1", "--id", REPORT_ID],
            vec!["--id", REPORT_ID, "--event", "push"],
            vec!["--pr", "1", "--event", "merge"],
            vec!["--pr", "-1"],
        ] {
            let output = hotpath(&server, Some(TOKEN), &report_args(&selector));
            assert_eq!(
                output.status.code(),
                Some(2),
                "{selector:?}: {}",
                stderr(&output)
            );
            assert_eq!(stdout(&output), "", "{selector:?}");
        }
        mock.assert();
    }
}
