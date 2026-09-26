#[cfg(all(test, feature = "cloud"))]
mod tests {
    //! `hotpath cloud auth` against a mock hotpath.rs: the bearer request it
    //! sends, the JSON it re-emits, every error code's message and exit 1,
    //! and that the token never reaches stdout or stderr.
    //!
    //! cargo test -p hotpath --features cloud --test cloud_cli

    use std::process::{Command, Output};

    use hotpath::json::cloud_api::{ApiError, ApiErrorCode, AuthStatus, TokenStatus};
    use mockito::{Matcher, Server, ServerGuard};
    use time::macros::datetime;

    const TOKEN: &str = "hpat_5f3c9a1b2d4e6f7a8b9c0d1e2f3a4b5c";
    const AUTH_BODY: &str =
        r#"{"login":"pawurb","token":{"name":"laptop","expires_at":"2027-01-01T00:00:00Z"}}"#;

    fn hotpath(server: &ServerGuard, token: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hotpath"));
        cmd.args(["cloud", "auth"])
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

    fn error_body(code: ApiErrorCode, error: &str) -> String {
        serde_json::to_string(&ApiError {
            error: error.into(),
            code,
        })
        .unwrap()
    }

    fn mock_auth(server: &mut ServerGuard) -> mockito::Mock {
        server
            .mock("GET", "/api/v1/auth")
            .match_header("authorization", format!("Bearer {TOKEN}").as_str())
            .match_header("user-agent", Matcher::Regex("^hotpath-cli/[0-9]".into()))
            .with_status(200)
            .with_header("content-type", "application/json; charset=utf-8")
            .with_body(AUTH_BODY)
            .create()
    }

    #[test]
    fn auth_prints_the_status_body_compact_by_default() {
        let mut server = Server::new();
        let mock = mock_auth(&mut server);

        let output = hotpath(&server, Some(TOKEN), &[]);
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

        let output = hotpath(&server, Some(TOKEN), &["--pretty"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        let expected: AuthStatus = serde_json::from_str(AUTH_BODY).unwrap();
        assert_eq!(
            stdout(&output),
            format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
        );

        let path = std::env::temp_dir().join("hotpath_cloud_cli_auth_output.json");
        let _ = std::fs::remove_file(&path);
        let output = hotpath(&server, Some(TOKEN), &["--output", path.to_str().unwrap()]);
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
            let output = hotpath(&server, token, &[]);
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(stdout(&output), "");
            let stderr = stderr(&output);
            assert!(stderr.contains("HOTPATH_API_TOKEN is not set"), "{stderr}");
            assert!(stderr.contains("https://hotpath.rs/app/tokens"), "{stderr}");
        }
        mock.assert();
    }

    #[test]
    fn auth_rejected_token_prints_the_sentence_and_request_id() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/api/v1/auth")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_header("www-authenticate", "Bearer")
            .with_header("x-request-id", "1bac4db9-15a")
            .with_body(error_body(
                ApiErrorCode::InvalidToken,
                "The token is unknown, expired or revoked.",
            ))
            .create();

        let output = hotpath(&server, Some(TOKEN), &[]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        assert_eq!(
            stderr(&output),
            "The token is unknown, expired or revoked. Check HOTPATH_API_TOKEN or create a new token at https://hotpath.rs/app/tokens. (request id: 1bac4db9-15a)\n"
        );
    }

    /// Status, code, server sentence, extra response headers, expected stderr.
    type ErrorCase = (
        u16,
        ApiErrorCode,
        &'static str,
        Vec<(&'static str, &'static str)>,
        &'static str,
    );

    #[test]
    fn auth_error_codes_add_their_hint() {
        let cases: [ErrorCase; 5] = [
            (
                401,
                ApiErrorCode::GithubAuthorizationExpired,
                "Your GitHub authorization expired.",
                vec![],
                "Your GitHub authorization expired. Log in at https://hotpath.rs/app once, then retry.\n",
            ),
            (
                429,
                ApiErrorCode::RateLimited,
                "Too many requests.",
                vec![("retry-after", "30")],
                "Too many requests. Retry after 30 s.\n",
            ),
            (
                404,
                ApiErrorCode::NotFound,
                "Not found.",
                vec![],
                "Not found.\n",
            ),
            (
                500,
                ApiErrorCode::Internal,
                "Something broke.",
                vec![("x-request-id", "abc")],
                "Something broke. (request id: abc)\n",
            ),
            (
                401,
                ApiErrorCode::Unknown,
                "A code this client does not know.",
                vec![],
                "A code this client does not know.\n",
            ),
        ];
        for (status, code, sentence, headers, expected) in cases {
            let mut server = Server::new();
            let body = if code == ApiErrorCode::Unknown {
                format!(r#"{{"error":"{sentence}","code":"quota_exceeded"}}"#)
            } else {
                error_body(code, sentence)
            };
            let mut mock = server
                .mock("GET", "/api/v1/auth")
                .with_status(status.into())
                .with_header("content-type", "application/json")
                .with_body(body);
            for (name, value) in headers {
                mock = mock.with_header(name, value);
            }
            let mock = mock.create();

            let output = hotpath(&server, Some(TOKEN), &[]);
            mock.assert();
            assert_eq!(output.status.code(), Some(1), "{code:?}");
            assert_eq!(stdout(&output), "", "{code:?}");
            assert_eq!(stderr(&output), expected, "{code:?}");
        }
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

        let output = hotpath(&server, Some(TOKEN), &[]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        assert_eq!(stderr(&output), format!("HTTP 502: {}...\n", &page[..200]));
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

        let output = hotpath(&server, Some(TOKEN), &[]);
        mock.assert();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        let stderr = stderr(&output);
        assert!(stderr.starts_with("invalid response from "), "{stderr}");
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
        let stderr = stderr(&output);
        assert!(
            stderr.starts_with(&format!("request to {url} failed: ")),
            "{stderr}"
        );
        assert!(!stderr.contains(TOKEN), "{stderr}");
    }
}
