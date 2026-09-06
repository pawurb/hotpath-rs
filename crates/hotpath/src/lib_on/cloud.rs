//! Uploads the JSON report to hotpath.rs from GitHub Actions, authenticated
//! with the job's OIDC token. Enabled at runtime by `HOTPATH_UPLOAD=1`; the
//! benchmark name comes from `HOTPATH_BENCHMARK` (default `default`, validated
//! by `validate_benchmark_name` - invalid names skip the upload). The target
//! base URL is `https://hotpath.rs` unless `HOTPATH_UPLOAD_URL` overrides it.
//!
//! `HOTPATH_UPLOAD_PATH` writes the payload to a file instead of posting it,
//! for a job that gets no OIDC token because it runs untrusted code: a fork
//! pull request writes the report, a trusted job uploads it. Either variable
//! builds the report at upload fidelity - every entry, plus histograms.
//!
//! Runs synchronously from the guard's `Drop`, after the runtime may already
//! be gone, so it never spawns tasks. Failures are reported on stderr and never
//! affect the process exit code.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;

use crate::json::JsonReport;

const DEFAULT_UPLOAD_URL: &str = "https://hotpath.rs";
const AUDIENCE: &str = "hotpath.rs";
const APP_URL: &str = "https://github.com/apps/hotpath-rs";
const MINT_TIMEOUT: Duration = Duration::from_secs(10);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct UploadResponse {
    pub(crate) id: String,
    pub(crate) repository: String,
    pub(crate) benchmark: String,
    pub(crate) baseline: Option<String>,
}

/// Base URL the report is posted to. `HOTPATH_UPLOAD_URL` overrides it for
/// staging or self-hosted backends; trailing slashes are trimmed so the
/// `/api/v1/reports` path is appended cleanly. Blank or unset falls back to
/// the default.
pub(crate) static UPLOAD_URL: LazyLock<String> =
    LazyLock::new(|| normalize_upload_url(std::env::var("HOTPATH_UPLOAD_URL").ok()));

fn normalize_upload_url(raw: Option<String>) -> String {
    raw.map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_UPLOAD_URL.to_string())
}

/// Path the report is written to, from `HOTPATH_UPLOAD_PATH`. Relative paths
/// resolve against the working directory, blank or unset writes nothing.
pub(crate) static UPLOAD_PATH: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| parse_upload_path(std::env::var("HOTPATH_UPLOAD_PATH").ok()));

fn parse_upload_path(raw: Option<String>) -> Option<PathBuf> {
    raw.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(crate::output_on::resolve_output_path)
}

pub(crate) fn post_enabled() -> bool {
    std::env::var("HOTPATH_UPLOAD")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

/// Either variable: a payload written for another job to post must match what
/// an in-process upload would have sent.
pub(crate) fn report_enabled() -> bool {
    post_enabled() || UPLOAD_PATH.is_some()
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

/// Writes the exact bytes `upload` would post, so a trusted job can forward the
/// file unchanged.
pub(crate) fn write_payload(path: &Path, report: &JsonReport) {
    let benchmark = match benchmark_name() {
        Ok(name) => name,
        Err(msg) => {
            eprintln!("hotpath: upload payload not written: {msg}");
            return;
        }
    };
    let result = serde_json::to_vec(report)
        .map_err(|e| format!("failed to serialize report: {e}"))
        .and_then(|body| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
            }
            std::fs::write(path, body)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))
        });

    match result {
        Ok(()) => eprintln!(
            "hotpath: wrote upload payload to {} (benchmark {benchmark})",
            path.display()
        ),
        Err(reason) => eprintln!("hotpath: upload payload not written: {reason}"),
    }
}

pub(crate) fn upload(report: &JsonReport) {
    if std::thread::panicking() {
        eprintln!("hotpath: upload skipped: the profiled program panicked");
        return;
    }
    let (request_url, request_token) = match (
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL"),
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN"),
    ) {
        (Ok(url), Ok(token)) if !url.is_empty() && !token.is_empty() => (url, token),
        _ => {
            eprintln!(
                "hotpath: upload skipped: not in GitHub Actions or missing `id-token: write` permission"
            );
            return;
        }
    };

    let benchmark = match benchmark_name() {
        Ok(name) => name,
        Err(msg) => {
            eprintln!("hotpath: upload skipped: {msg}");
            return;
        }
    };
    let result = mint_token(&request_url, &request_token).and_then(|token| {
        let body =
            serde_json::to_vec(report).map_err(|e| format!("failed to serialize report: {e}"))?;
        post_report(&UPLOAD_URL, &token, &benchmark, &body)
    });

    match result {
        Ok(resp) => eprintln!(
            "hotpath: uploaded report {} (repository {}, benchmark {}, baseline {})",
            resp.id,
            resp.repository,
            resp.benchmark,
            resp.baseline.as_deref().unwrap_or("none")
        ),
        Err(reason) => eprintln!("hotpath: upload failed: {reason}"),
    }
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

pub(crate) fn post_report(
    base_url: &str,
    token: &str,
    benchmark: &str,
    body: &[u8],
) -> Result<UploadResponse, String> {
    let url = format!(
        "{base_url}/api/v1/reports?benchmark={}",
        url_encode(benchmark)
    );
    let mut resp = agent(UPLOAD_TIMEOUT)
        .post(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("X-Hotpath-Version", env!("CARGO_PKG_VERSION"))
        .send(body)
        .map_err(|e| format!("request to {base_url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let body = read_body(&mut resp);
    if status != 201 {
        return Err(map_status(status, &body));
    }
    serde_json::from_str(&body).map_err(|e| format!("invalid server response: {e}"))
}

pub(crate) fn map_status(status: u16, body: &str) -> String {
    match status {
        401 => "server rejected the OIDC token (HTTP 401): expired or wrong audience".to_string(),
        403 => format!(
            "hotpath GitHub App is not installed on this repository (HTTP 403), install it at {APP_URL}"
        ),
        413 => "report exceeds the 5 MiB upload limit (HTTP 413)".to_string(),
        _ => format!("server returned HTTP {status}: {body}"),
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
    use crate::lib_on::cloud::{
        benchmark_name, is_truthy, map_status, normalize_upload_url, parse_upload_path, url_encode,
        validate_benchmark_name, APP_URL, DEFAULT_UPLOAD_URL,
    };

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
    fn upload_path_is_optional_and_absolute() {
        assert_eq!(parse_upload_path(None), None);
        assert_eq!(parse_upload_path(Some("   ".into())), None);
        let path = parse_upload_path(Some(" report.json ".into())).expect("path");
        assert!(path.is_absolute(), "{} should be absolute", path.display());
        assert!(path.ends_with("report.json"));
        #[cfg(unix)]
        assert_eq!(
            parse_upload_path(Some("/tmp/report.json".into())),
            Some(std::path::PathBuf::from("/tmp/report.json"))
        );
    }

    #[test]
    fn map_status_messages() {
        assert!(map_status(401, "").contains("401"));
        assert!(map_status(403, "").contains(APP_URL));
        assert!(map_status(413, "").contains("5 MiB"));
        assert!(map_status(500, "boom").contains("HTTP 500: boom"));
    }
}
