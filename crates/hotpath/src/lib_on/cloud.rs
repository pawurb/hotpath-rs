//! Uploads the JSON report to hotpath.rs from GitHub Actions, authenticated
//! with the job's OIDC token. Enabled at runtime by `HOTPATH_UPLOAD=1`; the
//! benchmark name comes from `HOTPATH_BENCHMARK` (default `default`, validated
//! by `validate_benchmark_name` - invalid names skip the upload). The target
//! base URL is `https://hotpath.rs` unless `HOTPATH_UPLOAD_URL` overrides it.
//!
//! Runs synchronously from the guard's `Drop`, after the runtime may already
//! be gone, so it never spawns tasks.
//!
//! Every run ends in one `Outcome`, rendered by `render` into one line at one
//! level and emitted by `emit`: always to stderr as `hotpath: <message>`, and
//! under GitHub Actions once more as a `::notice::` / `::warning::` /
//! `::error::` workflow command on stdout plus a block appended to
//! `GITHUB_STEP_SUMMARY`. The server owns the text: rejections are rendered
//! from the `UploadError` body (`error`, `hint`, `code`, `request_id`) and a
//! comment caveat from `comment.hint`; the client branches on nothing the
//! server says. A failure is a warning by default and never changes the exit
//! code; `HOTPATH_UPLOAD_STRICT=1` turns it into an error and exits 1 after
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

/// Base URL the report is posted to. `HOTPATH_UPLOAD_URL` overrides it for
/// staging or self-hosted backends; trailing slashes are trimmed so the
/// `/api/v1/reports` path is appended cleanly. Blank or unset falls back to
/// the default.
pub(crate) static UPLOAD_URL: LazyLock<String> =
    LazyLock::new(|| normalize_upload_url(std::env::var("HOTPATH_UPLOAD_URL").ok()));

/// `HOTPATH_UPLOAD_STRICT=1`: a failed upload is an `::error::` and exits 1.
/// Off by default so an adopter's benchmark job never goes red because
/// hotpath.rs is down.
pub(crate) static STRICT: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_UPLOAD_STRICT")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
});

fn normalize_upload_url(raw: Option<String>) -> String {
    raw.map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_UPLOAD_URL.to_string())
}

pub(crate) fn enabled() -> bool {
    std::env::var("HOTPATH_UPLOAD")
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
            "invalid HOTPATH_BENCHMARK {name:?}: use 1-64 chars from [A-Za-z0-9._-], not \".\" or \"..\""
        ));
    }
    Ok(())
}

pub(crate) fn benchmark_name() -> Result<String, String> {
    let name = match std::env::var("HOTPATH_BENCHMARK") {
        Ok(s) => s.trim().to_string(),
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("invalid HOTPATH_BENCHMARK: value is not valid UTF-8".to_string())
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
/// cannot be undone. Replaces `HOTPATH_LIMIT`, every `HOTPATH_<SECTION>_LIMIT`
/// and the builder limits for the upload only; unparsable values fall back
/// to `0`.
pub(crate) static UPLOAD_LIMIT: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("HOTPATH_UPLOAD_LIMIT")
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
    Failed(Failure),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Failure {
    /// Before hotpath.rs was contacted: report serialization or OIDC minting.
    /// The string says which.
    Local(String),
    /// The upload request did not complete (DNS, connect, TLS, read timeout).
    /// The server may or may not have seen it.
    Transport(String),
    /// Any answer with a status. `error` is the parsed body when it parsed as
    /// `UploadError`; otherwise `body` is rendered raw. A 2xx whose body does
    /// not parse as `UploadCreated` lands here with `error: None` and is
    /// reported as probably stored.
    Response {
        status: u16,
        /// From the `x-request-id` header, so it survives an empty or
        /// unparseable body.
        request_id: Option<String>,
        body: String,
        error: Option<UploadError>,
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
    pub(crate) level: Level,
    /// Printed to stderr as `hotpath: <message>` and, in Actions, once more
    /// as `::<level>::hotpath: <message>`.
    pub(crate) message: String,
    /// Markdown appended to `GITHUB_STEP_SUMMARY`.
    pub(crate) summary: String,
}

/// The bits of the process environment that decide how an outcome is shown.
pub(crate) struct Env {
    /// `GITHUB_ACTIONS` is set: emit workflow commands.
    pub(crate) actions: bool,
    pub(crate) strict: bool,
    /// `GITHUB_STEP_SUMMARY`, when set and non-empty.
    pub(crate) summary: Option<PathBuf>,
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
        Err(msg) => return Outcome::Failed(Failure::Local(msg)),
    };
    let body = match serde_json::to_vec(report) {
        Ok(body) => body,
        Err(e) => {
            return Outcome::Failed(Failure::Local(format!("failed to serialize report: {e}")))
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
            return Outcome::Failed(Failure::Transport(format!(
                "request to {base_url} failed: {e}"
            )))
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

/// Status plus body to an outcome. Pure, so the response shapes are testable
/// without a server: a 2xx parses as `UploadCreated`, anything else tries
/// `UploadError` and falls back to the raw body.
pub(crate) fn interpret(status: u16, request_id: Option<String>, body: String) -> Outcome {
    if (200..300).contains(&status) {
        return match serde_json::from_str::<UploadCreated>(&body) {
            Ok(created) => Outcome::Uploaded {
                created,
                request_id,
            },
            Err(_) => Outcome::Failed(Failure::Response {
                status,
                request_id,
                body,
                error: None,
            }),
        };
    }
    let error = serde_json::from_str::<UploadError>(&body).ok();
    Outcome::Failed(Failure::Response {
        status,
        request_id,
        body,
        error,
    })
}

/// One message at one level. Pure: `env` and `benchmark` are the only inputs
/// besides the outcome.
pub(crate) fn render(outcome: &Outcome, env: &Env, benchmark: Option<&str>) -> Rendered {
    let (level, message, json) = match outcome {
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
                request_suffix(request_id.as_deref()),
            );
            let level = match &created.comment.hint {
                Some(hint) => {
                    message.push_str(&format!(
                        "; comment {}: {hint}",
                        created.comment.reason.as_deref().unwrap_or("not posted")
                    ));
                    Level::Warning
                }
                None => Level::Notice,
            };
            let json = serde_json::to_string_pretty(created).ok();
            (level, message, json)
        }
        Outcome::Failed(failure) => {
            let level = if env.strict {
                Level::Error
            } else {
                Level::Warning
            };
            let (detail, json) = describe(failure);
            (level, format!("upload failed: {detail}"), json)
        }
    };

    let heading = match (outcome, benchmark) {
        (Outcome::Uploaded { created, .. }, _) => {
            format!("hotpath.rs {} benchmark", created.benchmark)
        }
        (_, Some(name)) => format!("hotpath.rs {name} benchmark"),
        (_, None) => "hotpath.rs upload".to_string(),
    };
    let mut summary = format!("## {heading}\n\n{}: hotpath: {message}\n", level.as_str());
    if let Some(json) = json {
        summary.push_str(&format!("\n```json\n{json}\n```\n"));
    }

    Rendered {
        level,
        message,
        summary,
    }
}

/// Message text for a failure plus, when there is a server body, its JSON
/// for the summary block.
fn describe(failure: &Failure) -> (String, Option<String>) {
    match failure {
        Failure::Local(msg) | Failure::Transport(msg) => (msg.clone(), None),
        Failure::Response {
            status,
            request_id,
            body,
            error: Some(error),
        } => {
            let mut detail = error.error.clone();
            if let Some(hint) = &error.hint {
                detail.push_str(&format!(". {hint}"));
            }
            detail.push_str(&format!(
                " ({}, HTTP {status}{})",
                error.code,
                request_suffix(error.request_id.as_deref().or(request_id.as_deref()))
            ));
            let json = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok());
            (detail, json)
        }
        Failure::Response {
            status,
            request_id,
            body,
            error: None,
        } => {
            let quoted = quote_body(body);
            let detail = if (200..300).contains(status) {
                format!(
                    "HTTP {status} but the response could not be read, the report was probably stored{}: {quoted}",
                    request_suffix(request_id.as_deref())
                )
            } else {
                format!(
                    "HTTP {status}{}: {quoted}",
                    request_suffix(request_id.as_deref())
                )
            };
            (detail, Some(body.clone()).filter(|b| !b.trim().is_empty()))
        }
    }
}

fn request_suffix(request_id: Option<&str>) -> String {
    request_id
        .map(|id| format!(", request {id}"))
        .unwrap_or_default()
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

/// Prints the rendered outcome. Workflow commands are read from stdout, the
/// human line goes to stderr as before. Exits 1 on `Level::Error`, which only
/// strict mode produces: this runs from `HotpathGuard::drop` and has no other
/// way to fail the job, so remaining destructors are skipped by design.
fn emit(rendered: Rendered, env: &Env) {
    eprintln!("hotpath: {}", rendered.message);
    if env.actions {
        println!(
            "::{}::hotpath: {}",
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
            eprintln!("hotpath: could not write {}: {e}", path.display());
        }
    }
    if rendered.level == Level::Error {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
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
        url_encode, validate_benchmark_name, Env, Failure, Level, Outcome, DEFAULT_UPLOAD_URL,
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

    // All HOTPATH_BENCHMARK cases live in one test so env access stays serialized.
    #[test]
    fn benchmark_name_from_env() {
        let var = "HOTPATH_BENCHMARK";
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

        // A 2xx whose body cannot be read is a failure that says "probably stored".
        let truncated = interpret(201, Some("abc".into()), r#"{"id":"r1","repo"#.into());
        assert_eq!(
            truncated,
            Outcome::Failed(Failure::Response {
                status: 201,
                request_id: Some("abc".into()),
                body: r#"{"id":"r1","repo"#.into(),
                error: None,
            })
        );
    }

    #[test]
    fn interpret_error_bodies() {
        let body = r#"{"code":"event_mismatch","error":"meta.ci.event is \"pull_request\" but the token was issued to a \"workflow_run\" run","hint":"Forward it through hotpath-relay.yml.","request_id":"1bac4db9-15a"}"#;
        match interpret(400, None, body.into()) {
            Outcome::Failed(Failure::Response {
                status: 400,
                error: Some(error),
                ..
            }) => {
                assert_eq!(error.code, "event_mismatch");
                assert_eq!(error.request_id.as_deref(), Some("1bac4db9-15a"));
            }
            other => panic!("expected parsed rejection, got {other:?}"),
        }

        // Unknown code: still parsed, rendered from its text.
        match interpret(
            418,
            None,
            r#"{"code":"teapot","error":"short and stout"}"#.into(),
        ) {
            Outcome::Failed(Failure::Response {
                error: Some(error), ..
            }) => assert_eq!(error.code, "teapot"),
            other => panic!("{other:?}"),
        }

        // Today's server: `{"error": ...}` without a code, kept raw.
        let legacy = interpret(400, None, r#"{"error":"invalid JSON"}"#.into());
        assert!(matches!(
            legacy,
            Outcome::Failed(Failure::Response {
                status: 400,
                error: None,
                ..
            })
        ));

        // Empty 408 from the router's timeout layer: only the header id survives.
        let timeout = interpret(408, Some("deadbeef".into()), String::new());
        assert_eq!(
            timeout,
            Outcome::Failed(Failure::Response {
                status: 408,
                request_id: Some("deadbeef".into()),
                body: String::new(),
                error: None,
            })
        );

        // A proxy's HTML.
        assert!(matches!(
            interpret(502, None, "<html>Bad Gateway</html>".into()),
            Outcome::Failed(Failure::Response {
                status: 502,
                error: None,
                ..
            })
        ));
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
        assert!(r
            .summary
            .starts_with("## hotpath.rs meta benchmark\n\nnotice: hotpath: uploaded report r1"));
        assert!(
            r.summary.contains("```json\n{\n  \"id\": \"r1\""),
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
    fn render_uploaded_with_comment_hint_is_one_warning() {
        let outcome = Outcome::Uploaded {
            created: UploadCreated {
                comment: CommentOutcome {
                    url: None,
                    reason: Some("permission_not_approved".into()),
                    hint: Some("approve \"Pull requests: write\" for the installation".into()),
                },
                ..created()
            },
            request_id: None,
        };
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Warning);
        assert_eq!(
            r.message,
            "uploaded report r1 (repository pawurb/hotpath-rs, benchmark meta, baseline r0); comment permission_not_approved: approve \"Pull requests: write\" for the installation"
        );

        // A caveat on a posted comment and a hint without a reason both render.
        let outcome = Outcome::Uploaded {
            created: UploadCreated {
                comment: CommentOutcome {
                    url: Some("https://github.com/c/1".into()),
                    reason: None,
                    hint: Some("the report does not parse".into()),
                },
                ..created()
            },
            request_id: None,
        };
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Warning);
        assert!(r
            .message
            .ends_with("; comment not posted: the report does not parse"));

        // A reason without a hint is the server saying there is nothing to show.
        let outcome = Outcome::Uploaded {
            created: UploadCreated {
                comment: CommentOutcome {
                    url: None,
                    reason: Some("not_a_pull_request".into()),
                    hint: None,
                },
                ..created()
            },
            request_id: None,
        };
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Notice);
        assert!(!r.message.contains("comment"));
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
                "## hotpath.rs meta benchmark\n\nnotice: hotpath: upload skipped: not in GitHub Actions or missing `id-token: write` permission\n"
            );
        }
        assert!(render(&outcome, &env(true, true), None)
            .summary
            .starts_with("## hotpath.rs upload\n"));
    }

    #[test]
    fn render_failed_levels() {
        let outcome = Outcome::Failed(Failure::Transport(
            "request to https://hotpath.rs failed: connection refused".into(),
        ));
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Warning);
        assert_eq!(
            r.message,
            "upload failed: request to https://hotpath.rs failed: connection refused"
        );
        assert_eq!(
            r.summary,
            "## hotpath.rs meta benchmark\n\nwarning: hotpath: upload failed: request to https://hotpath.rs failed: connection refused\n"
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

        let local = Outcome::Failed(Failure::Local(
            "OIDC token request returned HTTP 403: nope".into(),
        ));
        assert_eq!(
            render(&local, &env(true, false), Some("meta")).message,
            "upload failed: OIDC token request returned HTTP 403: nope"
        );
    }

    #[test]
    fn render_rejection_from_server_text() {
        let body = r#"{"code":"event_mismatch","error":"meta.ci.event is \"pull_request\" but the token was issued to a \"workflow_run\" run","hint":"Forward it through hotpath-relay.yml.","request_id":"1bac4db9-15a"}"#;
        let outcome = interpret(400, Some("1bac4db9-15a".into()), body.into());
        let r = render(&outcome, &env(true, false), Some("meta"));
        assert_eq!(r.level, Level::Warning);
        assert_eq!(
            r.message,
            "upload failed: meta.ci.event is \"pull_request\" but the token was issued to a \"workflow_run\" run. Forward it through hotpath-relay.yml. (event_mismatch, HTTP 400, request 1bac4db9-15a)"
        );
        assert!(
            r.summary
                .contains("```json\n{\n  \"code\": \"event_mismatch\""),
            "{}",
            r.summary
        );

        // No hint, no request id anywhere: still a readable line.
        let outcome = interpret(
            403,
            None,
            r#"{"code":"app_not_installed","error":"not installed"}"#.into(),
        );
        assert_eq!(
            render(&outcome, &env(true, true), Some("meta")).message,
            "upload failed: not installed (app_not_installed, HTTP 403)"
        );
        assert_eq!(
            render(&outcome, &env(true, true), Some("meta")).level,
            Level::Error
        );

        // Header id fills in when the body has none.
        let outcome = interpret(
            500,
            Some("hdr".into()),
            r#"{"code":"server_error","error":"database error"}"#.into(),
        );
        assert_eq!(
            render(&outcome, &env(true, false), Some("meta")).message,
            "upload failed: database error (server_error, HTTP 500, request hdr)"
        );
    }

    #[test]
    fn render_unreadable_responses() {
        let legacy = interpret(400, None, r#"{"error":"invalid JSON"}"#.into());
        assert_eq!(
            render(&legacy, &env(true, false), Some("meta")).message,
            r#"upload failed: HTTP 400: {"error":"invalid JSON"}"#
        );

        let timeout = interpret(408, Some("deadbeef".into()), String::new());
        let r = render(&timeout, &env(true, false), Some("meta"));
        assert_eq!(
            r.message,
            "upload failed: HTTP 408, request deadbeef: empty body"
        );
        assert!(
            !r.summary.contains("```"),
            "no fenced block for an empty body"
        );

        let stored = interpret(201, Some("abc".into()), "<html>".into());
        assert_eq!(
            render(&stored, &env(true, false), Some("meta")).message,
            "upload failed: HTTP 201 but the response could not be read, the report was probably stored, request abc: <html>"
        );

        let long = interpret(502, None, "x".repeat(2500));
        let message = render(&long, &env(true, false), Some("meta")).message;
        assert!(message.ends_with("... (2500 bytes)"), "{message}");
        assert!(message.len() < 2100);
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
