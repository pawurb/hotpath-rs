//! Shared plumbing of the `hotpath cloud` commands: the token and base URL
//! from the environment, the bearer `GET` / `POST` and the JSON output. The
//! token comes only from `HOTPATH_API_TOKEN` (never a flag, so it stays out of
//! shell history and `ps`) and is never printed, not even in an error. A 2xx
//! body the command only prints passes through as received (`RawBody`), so a
//! field this client does not know yet still reaches the caller; only the
//! parts a command branches on are typed (`hotpath::json::cloud_api`). Every
//! failure is one JSON document on stderr (`CliError`): a non-2xx server body
//! exactly as received - the client adds no hints, whatever the server says is
//! the whole advice - or `{"error": "..."}` built here when there is no such
//! body (token unset, network failure, unreadable body). Exit codes: 0 ok, 1 error
//! (any non-2xx, network failure, invalid argument value), 2 clap usage error
//! (clap's own, not remapped); `diff` also exits 1 when its body is not a
//! comparison without regressions (see `diff.rs`).

use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use hotpath::json::cloud_api::API_URL;
use serde::de::{DeserializeOwned, IgnoredAny};
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

/// A 2xx body that is valid JSON, kept exactly as received.
pub(crate) struct RawBody {
    url: String,
    body: String,
}

impl RawBody {
    /// The body parsed as `T`, the part of it a command branches on; failing
    /// like `Client::get` does.
    pub(crate) fn parse<T: DeserializeOwned>(&self) -> Result<T, CliError> {
        parse_body(&self.url, &self.body)
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
        let body = self.get_body(&url)?;
        parse_body(&url, &body)
    }

    /// `GET {base_url}{path}` like `get`, with the 2xx body kept as received:
    /// for a command that only prints it.
    pub(crate) fn get_raw(&self, path: &str) -> Result<RawBody, CliError> {
        let url = format!("{}{path}", self.base_url);
        let body = self.get_body(&url)?;
        parse_body::<IgnoredAny>(&url, &body)?;
        Ok(RawBody { url, body })
    }

    fn get_body(&self, url: &str) -> Result<String, CliError> {
        let resp = self
            .agent
            .get(url)
            .header("Authorization", &self.authorization())
            .call();
        self.read_response(url, resp)
    }

    /// `POST {base_url}{path}` with `body` as JSON and the bearer token; the
    /// answer is handled like `get`'s.
    pub(crate) fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, CliError> {
        let url = format!("{}{path}", self.base_url);
        let body = serde_json::to_vec(body)
            .map_err(|e| CliError::client(format!("could not serialize the request: {e}")))?;
        let resp = self
            .agent
            .post(&url)
            .header("Authorization", &self.authorization())
            .header("Content-Type", "application/json")
            .send(&body[..]);
        let body = self.read_response(&url, resp)?;
        parse_body(&url, &body)
    }

    fn authorization(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// A 2xx body as received, or the failure: the transport error (which
    /// names the base URL only) or the non-2xx body (see `CliError::server`).
    fn read_response(
        &self,
        url: &str,
        resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<String, CliError> {
        let mut resp = resp
            .map_err(|e| CliError::client(format!("request to {} failed: {e}", self.base_url)))?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string();
        if (200..300).contains(&status) {
            return body.map_err(|e| CliError::client(format!("invalid response from {url}: {e}")));
        }
        Err(CliError::server(status, body.unwrap_or_default()))
    }
}

/// A 2xx body of `url` parsed as `T`; a body that does not parse is an error,
/// never a default.
fn parse_body<T: DeserializeOwned>(url: &str, body: &str) -> Result<T, CliError> {
    serde_json::from_str(body)
        .map_err(|e| CliError::client(format!("invalid response from {url}: {e}")))
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
        let json = self
            .render(value)
            .map_err(|e| CliError::client(format!("could not serialize the response: {e}")))?;
        self.write(json)
    }

    /// Writes a body as the server sent it (see `render_raw`).
    pub(crate) fn emit_raw(&self, body: &RawBody) -> Result<(), CliError> {
        self.write(self.render_raw(&body.body))
    }

    pub(crate) fn emit_error(&self, error: &CliError) {
        let json = match error {
            CliError::Server(raw) => self.render_raw(raw),
            CliError::Client(message) => {
                let value = json!({ "error": message });
                self.render(&value).unwrap_or_else(|_| value.to_string())
            }
        };
        eprintln!("{json}");
    }

    fn write(&self, mut json: String) -> Result<(), CliError> {
        json.push('\n');
        match &self.file {
            Some(path) => std::fs::write(path, json)
                .map_err(|e| CliError::client(format!("could not write {}: {e}", path.display()))),
            None => std::io::stdout()
                .write_all(json.as_bytes())
                .map_err(|e| CliError::client(format!("could not write to stdout: {e}"))),
        }
    }

    /// A server body: compact output is the body exactly as received
    /// (trimmed), `--pretty` re-indents it in the server's key order
    /// (`serde_json/preserve_order` under the `cloud` feature).
    fn render_raw(&self, raw: &str) -> String {
        let raw = raw.trim();
        if !self.pretty {
            return raw.to_string();
        }
        serde_json::from_str::<Value>(raw)
            .and_then(|value| self.render(&value))
            .unwrap_or_else(|_| raw.to_string())
    }

    fn render<T: Serialize>(&self, value: &T) -> serde_json::Result<String> {
        if self.pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
    }
}
