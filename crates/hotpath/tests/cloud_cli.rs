#[cfg(all(test, feature = "cloud"))]
mod tests {
    //! `hotpath cloud auth|repos|benchmarks` against a mock hotpath.rs: the
    //! bearer request each sends, the JSON it re-emits, the error JSON on
    //! stderr (server bodies verbatim, client failures as `{"error": ...}`)
    //! with exit 1, the repository resolution of `benchmarks` (flag, else the
    //! `origin` remote of the working directory) and that the token never
    //! reaches stdout or stderr.
    //!
    //! cargo test -p hotpath --features cloud --test cloud_cli

    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use hotpath::json::cloud_api::{ApiError, ApiErrorCode, AuthStatus, RepoList, TokenStatus};
    use mockito::{Matcher, Server, ServerGuard};
    use time::macros::datetime;

    const TOKEN: &str = "hpat_5f3c9a1b2d4e6f7a8b9c0d1e2f3a4b5c";
    const AUTH_BODY: &str =
        r#"{"login":"pawurb","token":{"name":"laptop","expires_at":"2027-01-01T00:00:00Z"}}"#;
    const REPOS_BODY: &str = r#"{"repositories":[{"full_name":"pawurb/hotpath-rs","private":false,"visibility_public":true,"benchmarks":[{"name":"ci","reports":412,"latest_report_at":"2026-09-25T18:03:11Z"},{"name":"empty","reports":0,"latest_report_at":null}]},{"full_name":"pawurb/private-thing","private":true,"visibility_public":false,"benchmarks":[]}]}"#;
    const BENCHMARKS_BODY: &str = r#"{"repository":"pawurb/hotpath-rs","benchmarks":[{"name":"ci","reports":412,"latest_report_at":"2026-09-25T18:03:11Z"}]}"#;
    const BENCHMARKS_PATH: &str = "/api/v1/repos/pawurb/hotpath-rs/benchmarks";

    fn hotpath(server: &ServerGuard, token: Option<&str>, args: &[&str]) -> Output {
        hotpath_in(server, token, args, None)
    }

    /// Runs `hotpath cloud <args>`, from `dir` when given, asserting the
    /// token leaks into neither stream.
    fn hotpath_in(
        server: &ServerGuard,
        token: Option<&str>,
        args: &[&str],
        dir: Option<&Path>,
    ) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hotpath"));
        cmd.arg("cloud")
            .args(args)
            .env("HOTPATH_API_URL", format!("{}/", server.url()))
            .env_remove("HOTPATH_API_TOKEN");
        if let Some(token) = token {
            cmd.env("HOTPATH_API_TOKEN", token);
        }
        if let Some(dir) = dir {
            cmd.current_dir(dir);
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

    /// A fresh empty directory under the temp dir, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("hotpath-cloud-cli-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// `git init` plus an `origin` remote at `url`.
        fn git_repo(name: &str, url: &str) -> Self {
            let dir = Self::new(name);
            for args in [vec!["init", "-q"], vec!["remote", "add", "origin", url]] {
                let status = Command::new("git")
                    .args(&args)
                    .current_dir(&dir.0)
                    .status()
                    .expect("git is required by this test");
                assert!(
                    status.success(),
                    "git {args:?} failed in {}",
                    dir.0.display()
                );
            }
            dir
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
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
    fn benchmarks_with_repo_flag_requests_that_repository() {
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
    fn benchmarks_resolves_the_repository_from_the_origin_remote() {
        let cases = [
            ("scp", "git@github.com:pawurb/hotpath-rs.git"),
            ("https", "https://github.com/pawurb/hotpath-rs"),
            ("ssh", "ssh://git@github.com/pawurb/hotpath-rs.git"),
        ];
        for (name, url) in cases {
            let mut server = Server::new();
            let mock = mock_get(&mut server, BENCHMARKS_PATH, BENCHMARKS_BODY);
            let repo = TempDir::git_repo(name, url);

            let output = hotpath_in(&server, Some(TOKEN), &["benchmarks"], Some(repo.path()));
            mock.assert();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{url}: stderr: {}",
                stderr(&output)
            );
            assert_eq!(stdout(&output), format!("{BENCHMARKS_BODY}\n"), "{url}");
        }
    }

    #[test]
    fn benchmarks_without_a_usable_origin_asks_for_the_flag() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", Matcher::Regex("^/api/v1/repos/".into()))
            .expect(0)
            .create();

        let plain = TempDir::new("not-a-repo");
        let output = hotpath_in(&server, Some(TOKEN), &["benchmarks"], Some(plain.path()));
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let error = client_error(&output);
        assert!(error.contains("--repo"), "{error}");
        assert!(error.contains("origin"), "{error}");

        let gitlab = TempDir::git_repo("gitlab", "git@gitlab.com:pawurb/hotpath-rs.git");
        let output = hotpath_in(&server, Some(TOKEN), &["benchmarks"], Some(gitlab.path()));
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let error = client_error(&output);
        assert!(error.contains("--repo"), "{error}");
        assert!(
            error.contains("git@gitlab.com:pawurb/hotpath-rs.git"),
            "{error}"
        );
        assert!(error.contains("github.com"), "{error}");

        // An authenticated origin is named without its credential.
        let secret = "glpat-s3cr3t";
        let authenticated = TempDir::git_repo(
            "gitlab-token",
            &format!("https://oauth2:{secret}@gitlab.com/pawurb/hotpath-rs.git"),
        );
        let output = hotpath_in(
            &server,
            Some(TOKEN),
            &["benchmarks"],
            Some(authenticated.path()),
        );
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let error = client_error(&output);
        assert!(!error.contains(secret), "{error}");
        assert!(!error.contains("oauth2"), "{error}");
        assert!(
            error.contains("`https://gitlab.com/pawurb/hotpath-rs.git`"),
            "{error}"
        );

        mock.assert();
    }

    #[test]
    fn benchmarks_rejects_a_bad_repo_flag_without_a_request() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", Matcher::Regex("^/api/v1/repos/".into()))
            .expect(0)
            .create();

        // Even from a repository with a valid origin: a bad flag never falls
        // back to the remote.
        let repo = TempDir::git_repo("bad-flag", "git@github.com:pawurb/hotpath-rs.git");
        for value in [
            "nope", "a/b/c", "a/..", "./b", "a/", "/b", "a b/c", "a/b?x=1",
        ] {
            let output = hotpath_in(
                &server,
                Some(TOKEN),
                &["benchmarks", "--repo", value],
                Some(repo.path()),
            );
            assert_eq!(output.status.code(), Some(1), "{value}");
            assert_eq!(stdout(&output), "", "{value}");
            let error = client_error(&output);
            assert!(error.contains("invalid --repo"), "{value}: {error}");
            assert!(error.contains(value), "{value}: {error}");
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
}
