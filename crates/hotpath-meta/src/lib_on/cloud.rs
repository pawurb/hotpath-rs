//! Uploads the JSON report to hotpath.rs from GitHub Actions, authenticated
//! with the job's OIDC token. Enabled at runtime by `HOTPATH_META_UPLOAD=1`; the
//! benchmark name comes from `HOTPATH_META_BENCHMARK` (default `default`, validated
//! by `validate_benchmark_name` - invalid names skip the upload). The target
//! base URL is `https://hotpath.rs` unless `HOTPATH_META_UPLOAD_URL` overrides it.
//!
//! Runs synchronously from the guard's `Drop`, after the runtime may already
//! be gone, so it never spawns tasks.
//!
//! Every run ends in one `Outcome`, rendered by `render` into one line at one
//! level and emitted by `emit`: always to stderr as `hotpath-meta: <message>`, and
//! under GitHub Actions once more as a `::notice::` / `::warning::` /
//! `::error::` workflow command on stdout plus a block appended to
//! `GITHUB_STEP_SUMMARY`. The server owns the text: a rejection prints the
//! `error` sentence of the `UploadError` body (plus status and request id)
//! and a failed comment prints `comment.error`; the client branches on nothing
//! the server says. A failure is a warning by default and never changes the exit
//! code; `HOTPATH_META_UPLOAD_STRICT=1` turns it into an error and exits 1 after
//! the line is printed. Skips never fail, even in strict mode.
//!
//! No retries yet: a retry is only safe once the server insert is idempotent
//! per run, otherwise a timed-out upload that was in fact stored would be
//! duplicated.

use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;

use crate::json::cloud_api::{UploadCreated, UploadError};
use crate::json::JsonReport;

const DEFAULT_UPLOAD_URL: &str = "https://hotpath.rs";
const AUDIENCE: &str = "hotpath.rs";
const MINT_TIMEOUT: Duration = Duration::from_secs(10);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest raw (unparseable) response body quoted in a message.
const MAX_QUOTED_BODY: usize = 2000;

/// Base URL the report is posted to. `HOTPATH_META_UPLOAD_URL` overrides it for
/// staging or self-hosted backends; trailing slashes are trimmed so the
/// `/api/v1/reports` path is appended cleanly. Blank or unset falls back to
/// the default.
pub(crate) static UPLOAD_URL: LazyLock<String> =
    LazyLock::new(|| normalize_upload_url(std::env::var("HOTPATH_META_UPLOAD_URL").ok()));

/// `HOTPATH_META_UPLOAD_STRICT=1`: a failed upload is an `::error::` and exits 1.
/// Off by default so an adopter's benchmark job never goes red because
/// hotpath.rs is down.
pub(crate) static STRICT: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_UPLOAD_STRICT")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
});

fn normalize_upload_url(raw: Option<String>) -> String {
    raw.map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_UPLOAD_URL.to_string())
}

pub(crate) fn enabled() -> bool {
    std::env::var("HOTPATH_META_UPLOAD")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

fn is_truthy(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true")
}

/// Fails fast with a readable message before a token is minted and the report
/// serialized; the server enforces the same rule.
pub(crate) fn validate_benchmark_name(name: &str) -> Result<(), String> {
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    if name.is_empty() || name.len() > 64 || !valid_chars || name == "." || name == ".." {
        return Err(format!(
            "invalid HOTPATH_META_BENCHMARK {name:?}: use 1-64 chars from [A-Za-z0-9._-], not \".\" or \"..\""
        ));
    }
    Ok(())
}

pub(crate) fn benchmark_name() -> Result<String, String> {
    let name = match std::env::var("HOTPATH_META_BENCHMARK") {
        Ok(s) => s.trim().to_string(),
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("invalid HOTPATH_META_BENCHMARK: value is not valid UTF-8".to_string())
        }
    };
    if name.is_empty() {
        return Ok("default".to_string());
    }
    validate_benchmark_name(&name)?;
    Ok(name)
}

/// Per-section entry limit for the uploaded report. Defaults to `0`
/// (unlimited) so the server receives every measured entry: it diffs reports
/// by name and can truncate for display itself, while a client-side top-N
/// cannot be undone. Replaces `HOTPATH_META_LIMIT`, every `HOTPATH_META_<SECTION>_LIMIT`
/// and the builder limits for the upload only; unparsable values fall back
/// to `0`.
pub(crate) static UPLOAD_LIMIT: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_UPLOAD_LIMIT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
});

/// How one upload attempt ended.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    /// Nothing was sent and nothing is wrong with the server: not in Actions,
    /// no token, invalid benchmark name, the program panicked. Never fails,
    /// even in strict mode.
    Skipped(String),
    Uploaded {
        created: UploadCreated,
        /// From the `x-request-id` response header.
        request_id: Option<String>,
    },
    Failed {
        /// Built where the failure happened: the server's `error` sentence
        /// for a rejection, the transport error otherwise.
        message: String,
        /// The response body, when there was one, for the step summary.
        body: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Notice,
    Warning,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Notice => "notice",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }
}

/// One line at one level, plus the step summary block.
#[derive(Debug, PartialEq)]
pub(crate) struct Rendered {
    level: Level,
    /// Printed to stderr as `hotpath-meta: <message>` and, in Actions, once more
    /// as `::<level>::hotpath-meta: <message>`.
    message: String,
    /// Markdown appended to `GITHUB_STEP_SUMMARY`.
    summary: String,
}

pub(crate) struct Env {
    /// `GITHUB_ACTIONS` is set: emit workflow commands.
    actions: bool,
    strict: bool,
    /// `GITHUB_STEP_SUMMARY`, when set and non-empty.
    summary: Option<PathBuf>,
}

impl Env {
    fn from_process() -> Self {
        Env {
            actions: std::env::var("GITHUB_ACTIONS")
                .map(|v| is_truthy(&v))
                .unwrap_or(false),
            strict: *STRICT,
            summary: std::env::var_os("GITHUB_STEP_SUMMARY")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
        }
    }
}

pub(crate) fn upload(report: &JsonReport) {
    let outcome = run(report);
    let env = Env::from_process();
    let benchmark = benchmark_name().ok();
    emit(render(&outcome, &env, benchmark.as_deref()), &env);
}

fn run(report: &JsonReport) -> Outcome {
    if std::thread::panicking() {
        return Outcome::Skipped("the profiled program panicked".to_string());
    }
    let (request_url, request_token) = match (
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL"),
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN"),
    ) {
        (Ok(url), Ok(token)) if !url.is_empty() && !token.is_empty() => (url, token),
        _ => {
            return Outcome::Skipped(
                "not in GitHub Actions or missing `id-token: write` permission".to_string(),
            )
        }
    };
    let benchmark = match benchmark_name() {
        Ok(name) => name,
        Err(msg) => return Outcome::Skipped(msg),
    };
    let token = match mint_token(&request_url, &request_token) {
        Ok(token) => token,
        Err(message) => {
            return Outcome::Failed {
                message,
                body: None,
            }
        }
    };
    let body = match serde_json::to_vec(report) {
        Ok(body) => body,
        Err(e) => {
            return Outcome::Failed {
                message: format!("failed to serialize report: {e}"),
                body: None,
            }
        }
    };
    post_report(&UPLOAD_URL, &token, &benchmark, &body)
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

#[derive(Deserialize)]
struct TokenResponse {
    value: String,
}

pub(crate) fn mint_token(request_url: &str, request_token: &str) -> Result<String, String> {
    let separator = if request_url.contains('?') { '&' } else { '?' };
    let url = format!("{request_url}{separator}audience={AUDIENCE}");
    let mut resp = agent(MINT_TIMEOUT)
        .get(&url)
        .header("Authorization", &format!("bearer {request_token}"))
        .call()
        .map_err(|e| format!("OIDC token request failed: {e}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        let body = read_body(&mut resp);
        return Err(format!("OIDC token request returned HTTP {status}: {body}"));
    }
    let token: TokenResponse = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("invalid OIDC token response: {e}"))?;
    Ok(token.value)
}

pub(crate) fn post_report(base_url: &str, token: &str, benchmark: &str, body: &[u8]) -> Outcome {
    let url = format!(
        "{base_url}/api/v1/reports?benchmark={}",
        url_encode(benchmark)
    );
    let mut resp = match agent(UPLOAD_TIMEOUT)
        .post(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("X-Hotpath-Version", env!("CARGO_PKG_VERSION"))
        .send(body)
    {
        Ok(resp) => resp,
        Err(e) => {
            return Outcome::Failed {
                message: format!("request to {base_url} failed: {e}"),
                body: None,
            }
        }
    };
    let status = resp.status().as_u16();
    let request_id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = read_body(&mut resp);
    interpret(status, request_id, body)
}

/// A 201 (or the 200 of an already stored run) parses as `UploadCreated`,
/// anything else tries `UploadError` and falls back to quoting the raw body.
pub(crate) fn interpret(status: u16, request_id: Option<String>, body: String) -> Outcome {
    let request = |id: Option<&str>| id.map(|id| format!(", request {id}")).unwrap_or_default();
    if matches!(status, 200 | 201) {
        return match serde_json::from_str::<UploadCreated>(&body) {
            Ok(created) => Outcome::Uploaded {
                created,
                request_id,
            },
            Err(_) => Outcome::Failed {
                message: format!(
                    "HTTP {status} but the response could not be read, the report was probably stored{}: {}",
                    request(request_id.as_deref()),
                    quote_body(&body)
                ),
                body: Some(body),
            },
        };
    }
    let message = match serde_json::from_str::<UploadError>(&body) {
        Ok(error) => format!(
            "{} (HTTP {status}{})",
            error.error,
            request(error.request_id.as_deref().or(request_id.as_deref()))
        ),
        Err(_) => format!(
            "HTTP {status}{}: {}",
            request(request_id.as_deref()),
            quote_body(&body)
        ),
    };
    Outcome::Failed {
        message,
        body: Some(body),
    }
}

/// One message at one level.
pub(crate) fn render(outcome: &Outcome, env: &Env, benchmark: Option<&str>) -> Rendered {
    let (level, message, body) = match outcome {
        Outcome::Skipped(reason) => (Level::Notice, format!("upload skipped: {reason}"), None),
        Outcome::Uploaded {
            created,
            request_id,
        } => {
            let mut message = format!(
                "uploaded report {} (repository {}, benchmark {}, baseline {}{})",
                created.id,
                created.repository,
                created.benchmark,
                created.baseline.as_deref().unwrap_or("none"),
                request_id
                    .as_deref()
                    .map(|id| format!(", request {id}"))
                    .unwrap_or_default(),
            );
            let level = match &created.comment.error {
                Some(error) => {
                    message.push_str(&format!("; comment failed: {error}"));
                    Level::Warning
                }
                None => Level::Notice,
            };
            (level, message, serde_json::to_string_pretty(created).ok())
        }
        Outcome::Failed { message, body } => {
            let level = if env.strict {
                Level::Error
            } else {
                Level::Warning
            };
            let body = body.as_deref().filter(|b| !b.trim().is_empty());
            (
                level,
                format!("upload failed: {message}"),
                body.map(str::to_string),
            )
        }
    };

    let heading = match benchmark {
        Some(name) => format!("hotpath.rs {name} benchmark"),
        None => "hotpath.rs upload".to_string(),
    };
    let mut summary = format!(
        "## {heading}\n\n{}: hotpath-meta: {message}\n",
        level.as_str()
    );
    if let Some(body) = body {
        summary.push_str(&format!("\n```\n{body}\n```\n"));
    }

    Rendered {
        level,
        message,
        summary,
    }
}

fn quote_body(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return "empty body".to_string();
    }
    match body.char_indices().nth(MAX_QUOTED_BODY) {
        Some((cut, _)) => format!("{}... ({} bytes)", &body[..cut], body.len()),
        None => body.to_string(),
    }
}

/// Escapes the message part of a workflow command (`::error::<message>`).
/// Unescaped, a newline ends the command and the rest of the text is lost.
pub(crate) fn escape_annotation(message: &str) -> String {
    message
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Workflow commands are read from stdout, the human line goes to stderr as
/// before. Exits 1 on `Level::Error` (strict mode only): this runs from
/// `HotpathGuard::drop` and has no other way to fail the job.
fn emit(rendered: Rendered, env: &Env) {
    eprintln!("hotpath-meta: {}", rendered.message);
    if env.actions {
        println!(
            "::{}::hotpath-meta: {}",
            rendered.level.as_str(),
            escape_annotation(&rendered.message)
        );
    }
    if let Some(path) = &env.summary {
        let appended = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .and_then(|mut f| f.write_all(rendered.summary.as_bytes()));
        if let Err(e) = appended {
            eprintln!("hotpath-meta: could not write {}: {e}", path.display());
        }
    }
    if rendered.level == Level::Error {
        let _ = std::io::stdout().flush();
        std::process::exit(1);
    }
}

fn read_body(resp: &mut ureq::http::Response<ureq::Body>) -> String {
    resp.body_mut().read_to_string().unwrap_or_default()
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::json::cloud_api::{CommentOutcome, UploadCreated};
    use crate::lib_on::cloud::{
        benchmark_name, escape_annotation, interpret, is_truthy, normalize_upload_url, render,
        url_encode, validate_benchmark_name, Env, Level, Outcome, DEFAULT_UPLOAD_URL,
    };

    fn env(actions: bool, strict: bool) -> Env {
        Env {
            actions,
            strict,
            summary: None,
        }
    }

    fn created() -> UploadCreated {
        UploadCreated {
            id: "r1".into(),
            repository: "pawurb/hotpath-rs".into(),
            benchmark: "meta".into(),
            baseline: Some("r0".into()),
            comment: CommentOutcome::default(),
        }
    }

    #[test]
    fn truthy_values() {
        assert!(is_truthy("1"));
        assert!(is_truthy("true"));
        assert!(is_truthy(" TRUE "));
        assert!(!is_truthy("0"));
        assert!(!is_truthy("false"));
        assert!(!is_truthy(""));
    }

    #[test]
    fn upload_url_override() {
        assert_eq!(normalize_upload_url(None), DEFAULT_UPLOAD_URL);
        assert_eq!(normalize_upload_url(Some("   ".into())), DEFAULT_UPLOAD_URL);
        assert_eq!(
            normalize_upload_url(Some(" http://localhost:3000/// ".into())),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_upload_url(Some("https://staging.hotpath.rs".into())),
            "https://staging.hotpath.rs"
        );
    }

    #[test]
    fn url_encode_escapes_reserved_chars() {
        assert_eq!(url_encode("timing-linux_1.0"), "timing-linux_1.0");
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn validate_benchmark_name_rule() {
        for ok in [
            "default",
            "ci",
            "timing-linux",
            "api_latency",
            "v0.25",
            "timing.linux",
        ] {
            assert!(
                validate_benchmark_name(ok).is_ok(),
                "{ok:?} should be valid"
            );
        }
        for bad in ["a/b", "a b", "..", ".", "x?y", "ünïcode", ""] {
            assert!(
                validate_benchmark_name(bad).is_err(),
                "{bad:?} should be invalid"
            );
        }
        assert!(validate_benchmark_name(&"a".repeat(64)).is_ok());
        assert!(validate_benchmark_name(&"a".repeat(65)).is_err());

        let err = validate_benchmark_name("a/b").unwrap_err();
        assert!(err.contains("\"a/b\""), "message names the value: {err}");
        assert!(
            err.contains("[A-Za-z0-9._-]"),
            "message names the rule: {err}"
        );
    }

    // All HOTPATH_META_BENCHMARK cases live in one test so env access stays serialized.
    #[test]
    fn benchmark_name_from_env() {
        let var = "HOTPATH_META_BENCHMARK";
        std::env::remove_var(var);
        assert_eq!(benchmark_name(), Ok("default".to_string()));
        std::env::set_var(var, "  ");
        assert_eq!(benchmark_name(), Ok("default".to_string()));
        std::env::set_var(var, " ci ");
        assert_eq!(benchmark_name(), Ok("ci".to_string()));
        std::env::set_var(var, "a/b");
        assert!(benchmark_name().is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            std::env::set_var(var, std::ffi::OsStr::from_bytes(b"\xFF\xFE"));
            let err = benchmark_name().unwrap_err();
            assert!(err.contains("not valid UTF-8"), "{err}");
        }
        std::env::remove_var(var);
    }

    #[test]
    fn interpret_success_bodies() {
        let ok = interpret(
            201,
            Some("abc".into()),
            r#"{"id":"r1","repository":"a/b","benchmark":"meta","comment":{"url":"https://github.com/c/1"}}"#.into(),
        );
        match ok {
            Outcome::Uploaded {
                created,
                request_id,
            } => {
                assert_eq!(created.id, "r1");
                assert_eq!(
                    created.comment.url.as_deref(),
                    Some("https://github.com/c/1")
                );
                assert_eq!(request_id.as_deref(), Some("abc"));
            }
            other => panic!("expected Uploaded, got {other:?}"),
        }

        // 200: the server already had this run.
        assert!(matches!(
            interpret(
                200,
                None,
                r#"{"id":"r1","repository":"a/b","benchmark":"meta"}"#.into()
            ),
            Outcome::Uploaded { .. }
        ));

        // Any other status is a failure even with a success-shaped body.
        assert_eq!(
            interpret(
                202,
                None,
                r#"{"id":"r1","repository":"a/b","benchmark":"meta"}"#.into()
            ),
            Outcome::Failed {
                message: r#"HTTP 202: {"id":"r1","repository":"a/b","benchmark":"meta"}"#.into(),
                body: Some(r#"{"id":"r1","repository":"a/b","benchmark":"meta"}"#.into()),
            }
        );

        // A success status whose body cannot be read is a failure that says "probably stored".
        assert_eq!(
            interpret(201, Some("abc".into()), r#"{"id":"r1","repo"#.into()),
            Outcome::Failed {
                message: r#"HTTP 201 but the response could not be read, the report was probably stored, request abc: {"id":"r1","repo"#.into(),
                body: Some(r#"{"id":"r1","repo"#.into()),
            }
        );
    }

    #[test]
    fn interpret_error_bodies() {
        let body = r#"{"error":"meta.ci.event is \"pull_request\" but the token was issued to a \"workflow_run\" run. Forward it through hotpath-relay.yml.","request_id":"1bac4db9-15a"}"#;
        assert_eq!(
            interpret(400, None, body.into()),
            Outcome::Failed {
                message: "meta.ci.event is \"pull_request\" but the token was issued to a \"workflow_run\" run. Forward it through hotpath-relay.yml. (HTTP 400, request 1bac4db9-15a)".into(),
                body: Some(body.into()),
            }
        );

        // The header id fills in when the body has none; extra fields are ignored.
        assert_eq!(
            interpret(
                500,
                Some("hdr".into()),
                r#"{"error":"database error","code":"x"}"#.into()
            ),
            Outcome::Failed {
                message: "database error (HTTP 500, request hdr)".into(),
                body: Some(r#"{"error":"database error","code":"x"}"#.into()),
            }
        );

        // Empty 408 from the router's timeout layer: only the header id survives.
        assert_eq!(
            interpret(408, Some("deadbeef".into()), String::new()),
            Outcome::Failed {
                message: "HTTP 408, request deadbeef: empty body".into(),
                body: Some(String::new()),
            }
        );

        // A proxy's HTML, and a body too long to quote whole.
        assert_eq!(
            interpret(502, None, "<html>Bad Gateway</html>".into()),
            Outcome::Failed {
                message: "HTTP 502: <html>Bad Gateway</html>".into(),
                body: Some("<html>Bad Gateway</html>".into()),
            }
        );
        let Outcome::Failed { message, .. } = interpret(502, None, "x".repeat(2500)) else {
            panic!()
        };
        assert!(message.ends_with("... (2500 bytes)"), "{message}");
        assert!(message.len() < 2100);
    }

    #[test]
    fn render_uploaded() {
        let outcome = Outcome::Uploaded {
            created: created(),
            request_id: Some("abc".into()),
        };
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Notice);
        assert_eq!(
            r.message,
            "uploaded report r1 (repository pawurb/hotpath-rs, benchmark meta, baseline r0, request abc)"
        );
        assert!(r.summary.starts_with(
            "## hotpath.rs meta benchmark\n\nnotice: hotpath-meta: uploaded report r1"
        ));
        assert!(
            r.summary.contains("```\n{\n  \"id\": \"r1\""),
            "{}",
            r.summary
        );

        // Strict mode changes nothing on success.
        assert_eq!(
            render(&outcome, &env(true, true), Some("meta")).level,
            Level::Notice
        );

        let no_baseline = Outcome::Uploaded {
            created: UploadCreated {
                baseline: None,
                ..created()
            },
            request_id: None,
        };
        assert_eq!(
            render(&no_baseline, &env(false, false), None).message,
            "uploaded report r1 (repository pawurb/hotpath-rs, benchmark meta, baseline none)"
        );
    }

    #[test]
    fn render_uploaded_with_comment_error_is_one_warning() {
        let outcome = Outcome::Uploaded {
            created: UploadCreated {
                comment: CommentOutcome {
                    url: None,
                    error: Some("approve \"Pull requests: write\" for the installation".into()),
                },
                ..created()
            },
            request_id: None,
        };
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Warning);
        assert_eq!(
            r.message,
            "uploaded report r1 (repository pawurb/hotpath-rs, benchmark meta, baseline r0); comment failed: approve \"Pull requests: write\" for the installation"
        );

        // A comment that was posted, or nothing to post, is a plain notice.
        for comment in [
            CommentOutcome {
                url: Some("https://github.com/c/1".into()),
                error: None,
            },
            CommentOutcome::default(),
        ] {
            let outcome = Outcome::Uploaded {
                created: UploadCreated {
                    comment,
                    ..created()
                },
                request_id: None,
            };
            let r = render(&outcome, &env(true, false), Some("meta"));
            assert_eq!(r.level, Level::Notice);
            assert!(!r.message.contains("comment"));
        }
    }

    #[test]
    fn render_skipped_never_fails() {
        let outcome = Outcome::Skipped(
            "not in GitHub Actions or missing `id-token: write` permission".into(),
        );
        for (actions, strict) in [(false, false), (true, false), (true, true)] {
            let r = render(&outcome, &env(actions, strict), Some("meta"));
            assert_eq!(r.level, Level::Notice);
            assert_eq!(
                r.message,
                "upload skipped: not in GitHub Actions or missing `id-token: write` permission"
            );
            assert_eq!(
                r.summary,
                "## hotpath.rs meta benchmark\n\nnotice: hotpath-meta: upload skipped: not in GitHub Actions or missing `id-token: write` permission\n"
            );
        }
        assert!(render(&outcome, &env(true, true), None)
            .summary
            .starts_with("## hotpath.rs upload\n"));
    }

    #[test]
    fn render_failed_levels() {
        let outcome = Outcome::Failed {
            message: "request to https://hotpath.rs failed: connection refused".into(),
            body: None,
        };
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Warning);
        assert_eq!(
            r.message,
            "upload failed: request to https://hotpath.rs failed: connection refused"
        );
        assert_eq!(
            r.summary,
            "## hotpath.rs meta benchmark\n\nwarning: hotpath-meta: upload failed: request to https://hotpath.rs failed: connection refused\n"
        );
        assert_eq!(
            render(&outcome, &env(true, true), Some("meta")).level,
            Level::Error
        );
        // Outside Actions the level still follows strict; only emission differs.
        assert_eq!(
            render(&outcome, &env(false, false), Some("meta")).level,
            Level::Warning
        );
        assert_eq!(
            render(&outcome, &env(false, true), Some("meta")).level,
            Level::Error
        );

        // A server body goes into the summary's fenced block; an empty one does not.
        let rejected = interpret(403, None, r#"{"error":"not installed"}"#.into());
        let r = render(&rejected, &env(true, false), Some("meta"));
        assert_eq!(r.message, "upload failed: not installed (HTTP 403)");
        assert!(
            r.summary
                .ends_with("\n```\n{\"error\":\"not installed\"}\n```\n"),
            "{}",
            r.summary
        );
        let timeout = interpret(408, Some("deadbeef".into()), String::new());
        assert!(!render(&timeout, &env(true, false), Some("meta"))
            .summary
            .contains("```"));
    }

    #[test]
    fn escape_annotation_keeps_one_line() {
        assert_eq!(escape_annotation("plain"), "plain");
        assert_eq!(
            escape_annotation("100% of\r\nlines\nbreak"),
            "100%25 of%0D%0Alines%0Abreak"
        );
    }
}
