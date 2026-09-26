//! Shared plumbing of the `hotpath cloud` commands: the token and base URL
//! from the environment, the bearer `GET`, `ApiError` handling and the JSON
//! output. The token comes only from `HOTPATH_API_TOKEN` (never a flag, so it
//! stays out of shell history and `ps`) and is never printed, not even in an
//! error.
//!
//! Exit codes: 0 ok, 1 error (any non-2xx, network failure, bad arguments).
//! `diff` will add 2 (no baseline / unreadable) and 3 (regression).

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use hotpath::json::cloud_api::{normalize_base_url, ApiError, ApiErrorCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub const TOKENS_URL: &str = "https://hotpath.rs/app/tokens";
const LOGIN_URL: &str = "https://hotpath.rs/app";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Longest raw (unparseable) response body quoted in a message.
const MAX_QUOTED_BODY: usize = 200;

pub struct Client {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl Client {
    pub fn from_env() -> Result<Self, String> {
        let token = std::env::var("HOTPATH_API_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                format!(
                    "HOTPATH_API_TOKEN is not set. Create a token at {TOKENS_URL} and export it."
                )
            })?;
        let base_url = normalize_base_url(std::env::var("HOTPATH_API_URL").ok());
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .user_agent(concat!("hotpath-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Self {
            base_url,
            token,
            agent,
        })
    }

    /// `GET {base_url}{path}` with the bearer token; a 2xx body parses as `T`,
    /// anything else is the `ApiError` sentence (plus the request id) as the
    /// error, or `HTTP <status>: <body>` when the body is not that JSON.
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = format!("{}{path}", self.base_url);
        let mut resp = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map_err(|e| format!("request to {} failed: {e}", self.base_url))?;
        let status = resp.status().as_u16();
        let header = |name: &str| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        if (200..300).contains(&status) {
            return resp
                .body_mut()
                .read_json::<T>()
                .map_err(|e| format!("invalid response from {url}: {e}"));
        }
        let request_id = header("x-request-id");
        let retry_after = header("retry-after");
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        Err(explain(status, &body, request_id, retry_after))
    }
}

/// The message for a non-2xx answer. Branches on `code`, never on the
/// sentence; an unknown code prints like any other error.
fn explain(
    status: u16,
    body: &str,
    request_id: Option<String>,
    retry_after: Option<String>,
) -> String {
    let Ok(error) = serde_json::from_str::<ApiError>(body) else {
        return format!("HTTP {status}: {}", quote_body(body));
    };
    let mut message = error.error.trim().to_string();
    let hint = match error.code {
        ApiErrorCode::MissingToken | ApiErrorCode::InvalidToken => Some(format!(
            "Check HOTPATH_API_TOKEN or create a new token at {TOKENS_URL}."
        )),
        ApiErrorCode::GithubAuthorizationExpired => {
            Some(format!("Log in at {LOGIN_URL} once, then retry."))
        }
        ApiErrorCode::RateLimited => Some(match retry_after {
            Some(seconds) => format!("Retry after {seconds} s."),
            None => "Retry later.".to_string(),
        }),
        ApiErrorCode::BadRequest
        | ApiErrorCode::NotFound
        | ApiErrorCode::MethodNotAllowed
        | ApiErrorCode::Internal
        | ApiErrorCode::Unknown => None,
    };
    if let Some(hint) = hint {
        message.push(' ');
        message.push_str(&hint);
    }
    if let Some(id) = request_id {
        message.push_str(&format!(" (request id: {id})"));
    }
    message
}

fn quote_body(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return "empty response body".to_string();
    }
    let end = hotpath::floor_char_boundary(body, MAX_QUOTED_BODY);
    if end < body.len() {
        format!("{}...", &body[..end])
    } else {
        body.to_string()
    }
}

/// Where and how a command's JSON goes: compact on stdout unless `--pretty`
/// or `--output FILE` say otherwise.
pub struct Output {
    pub pretty: bool,
    pub file: Option<PathBuf>,
}

impl Output {
    pub fn emit<T: Serialize>(&self, value: &T) -> Result<(), String> {
        let mut json = if self.pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
        .map_err(|e| format!("could not serialize the response: {e}"))?;
        json.push('\n');
        match &self.file {
            Some(path) => std::fs::write(path, json)
                .map_err(|e| format!("could not write {}: {e}", path.display())),
            None => std::io::stdout()
                .write_all(json.as_bytes())
                .map_err(|e| format!("could not write to stdout: {e}")),
        }
    }
}
