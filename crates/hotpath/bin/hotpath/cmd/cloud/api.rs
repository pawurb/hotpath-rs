//! Shared plumbing of the `hotpath cloud` commands: the token and base URL
//! from the environment, the bearer `GET` and the JSON output. The token comes
//! only from `HOTPATH_API_TOKEN` (never a flag, so it stays out of shell
//! history and `ps`) and is never printed, not even in an error. Every failure
//! is one JSON document on stderr (`CliError`): a non-2xx server body exactly
//! as received - the client adds no hints, whatever the server says is the
//! whole advice - or `{"error": "..."}` built here when there is no such body
//! (token unset, network failure, unreadable body). Exit codes: 0 ok, 1 error
//! (any non-2xx, network failure, bad arguments).

use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use hotpath::json::cloud_api::normalize_base_url;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

pub(crate) const TOKENS_URL: &str = "https://hotpath.rs/app/tokens";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Longest raw (unparseable) response body quoted in a message.
const MAX_QUOTED_BODY: usize = 200;

/// `HOTPATH_API_TOKEN`, trimmed; `None` when unset or blank.
static API_TOKEN: LazyLock<Option<String>> = LazyLock::new(|| {
    std::env::var("HOTPATH_API_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
});

/// `HOTPATH_API_URL`, normalized like `HOTPATH_UPLOAD_URL`.
static API_URL: LazyLock<String> =
    LazyLock::new(|| normalize_base_url(std::env::var("HOTPATH_API_URL").ok()));

/// One failure of a `hotpath cloud` command, rendered by `Output::emit_error`.
#[derive(Debug)]
pub(crate) enum CliError {
    /// A non-2xx server body that is valid JSON, kept verbatim so compact
    /// output prints exactly what the server sent.
    Server(String),
    /// A failure with no server body to show; prints as `{"error": message}`.
    Client(String),
}

impl CliError {
    pub(crate) fn client(message: impl Into<String>) -> Self {
        Self::Client(message.into())
    }

    /// A non-2xx body: passed through when it is JSON, otherwise quoted
    /// with the status.
    fn server(status: u16, body: String) -> Self {
        match serde_json::from_str::<Value>(&body) {
            Ok(_) => Self::Server(body.trim().to_string()),
            Err(_) => Self::client(format!("HTTP {status}: {}", quote_body(&body))),
        }
    }
}

pub(crate) struct Client {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl Client {
    pub(crate) fn from_env() -> Result<Self, CliError> {
        let token = API_TOKEN.clone().ok_or_else(|| {
            CliError::client(format!(
                "HOTPATH_API_TOKEN is not set. Create a token at {TOKENS_URL} and export it."
            ))
        })?;
        let base_url = API_URL.clone();
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
    /// anything else is the server body as the error (see `CliError::server`).
    pub(crate) fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CliError> {
        let url = format!("{}{path}", self.base_url);
        let mut resp = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map_err(|e| CliError::client(format!("request to {} failed: {e}", self.base_url)))?;
        let status = resp.status().as_u16();
        if (200..300).contains(&status) {
            return resp
                .body_mut()
                .read_json::<T>()
                .map_err(|e| CliError::client(format!("invalid response from {url}: {e}")));
        }
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        Err(CliError::server(status, body))
    }
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
/// or `--output FILE` say otherwise. Errors always go to stderr, indented
/// under `--pretty` like a body.
pub(crate) struct Output {
    pub(crate) pretty: bool,
    pub(crate) file: Option<PathBuf>,
}

impl Output {
    pub(crate) fn emit<T: Serialize>(&self, value: &T) -> Result<(), CliError> {
        let mut json = self
            .render(value)
            .map_err(|e| CliError::client(format!("could not serialize the response: {e}")))?;
        json.push('\n');
        match &self.file {
            Some(path) => std::fs::write(path, json)
                .map_err(|e| CliError::client(format!("could not write {}: {e}", path.display()))),
            None => std::io::stdout()
                .write_all(json.as_bytes())
                .map_err(|e| CliError::client(format!("could not write to stdout: {e}"))),
        }
    }

    pub(crate) fn emit_error(&self, error: &CliError) {
        let json = match error {
            CliError::Server(raw) if !self.pretty => raw.clone(),
            CliError::Server(raw) => serde_json::from_str::<Value>(raw)
                .and_then(|value| self.render(&value))
                .unwrap_or_else(|_| raw.clone()),
            CliError::Client(message) => {
                let value = json!({ "error": message });
                self.render(&value).unwrap_or_else(|_| value.to_string())
            }
        };
        eprintln!("{json}");
    }

    fn render<T: Serialize>(&self, value: &T) -> serde_json::Result<String> {
        if self.pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
    }
}
