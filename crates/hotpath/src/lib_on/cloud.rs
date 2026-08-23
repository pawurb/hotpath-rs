//! Uploads the JSON report to hotpath.rs from GitHub Actions, authenticated
//! with the job's OIDC token. Enabled at runtime by `HOTPATH_UPLOAD=1`; the
//! benchmark name comes from `HOTPATH_BENCHMARK` (default `default`).
//!
//! Runs synchronously from the guard's `Drop`, after the runtime may already
//! be gone, so it never spawns tasks. Failures are reported on stderr and never
//! affect the process exit code.

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

pub(crate) fn enabled() -> bool {
    std::env::var("HOTPATH_UPLOAD")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

fn is_truthy(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true")
}

pub(crate) fn benchmark_name() -> String {
    std::env::var("HOTPATH_BENCHMARK")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

// Internal override used by integration tests and hotpath-backend's CI.
pub(crate) fn upload_url() -> String {
    std::env::var("HOTPATH_UPLOAD_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_UPLOAD_URL.to_string())
}

pub(crate) fn upload(report: &JsonReport) {
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

    let benchmark = benchmark_name();
    let result = mint_token(&request_url, &request_token).and_then(|token| {
        let body =
            serde_json::to_vec(report).map_err(|e| format!("failed to serialize report: {e}"))?;
        post_report(&upload_url(), &token, &benchmark, &body)
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
        return Err(format!(
            "OIDC token request returned HTTP {status}: {}",
            truncate(&body)
        ));
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
        400 => format!("server rejected the report (HTTP 400): {}", truncate(body)),
        _ => format!("server returned HTTP {status}: {}", truncate(body)),
    }
}

fn read_body(resp: &mut ureq::http::Response<ureq::Body>) -> String {
    resp.body_mut().read_to_string().unwrap_or_default()
}

fn truncate(s: &str) -> String {
    const MAX: usize = 200;
    let s = s.trim();
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(MAX).collect::<String>())
    }
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
    use crate::lib_on::cloud::{is_truthy, map_status, url_encode, APP_URL};

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
    fn url_encode_escapes_reserved_chars() {
        assert_eq!(url_encode("timing-linux_1.0"), "timing-linux_1.0");
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn map_status_messages() {
        assert!(map_status(401, "").contains("401"));
        assert!(map_status(403, "").contains(APP_URL));
        assert!(map_status(413, "").contains("5 MiB"));
        assert!(map_status(400, "bad").contains("bad"));
        assert!(map_status(500, "boom").contains("HTTP 500: boom"));
    }
}
