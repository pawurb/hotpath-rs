//! Wire types of the hotpath.rs API, shared by the `hotpath-cloud` upload
//! client (`lib_on/cloud.rs`), the `hotpath cloud` CLI (`cloud` binary
//! feature) and the hotpath-backend server, which depends on this crate with
//! the `json` feature. Every body the server sends is defined here first;
//! the server serializes these types and keeps no structs of its own.
//!
//! Deliberately minimal: the server owns every sentence, the clients print it
//! and branch only on `ApiError::code`.

use serde::{Deserialize, Serialize};

/// Base URL of the hotpath.rs API when nothing overrides it.
pub const DEFAULT_BASE_URL: &str = "https://hotpath.rs";

/// Turns the raw value of `HOTPATH_UPLOAD_URL` / `HOTPATH_API_URL` into a
/// base URL: trimmed, without trailing slashes, `DEFAULT_BASE_URL` when unset
/// or blank.
pub fn normalize_base_url(raw: Option<String>) -> String {
    raw.map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Why a request was refused, as clients branch on it. Never on the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    /// No `Authorization: Bearer` credential at all (401).
    MissingToken,
    /// Unknown, expired or revoked token: one code, so a probe learns nothing (401).
    InvalidToken,
    /// The user's GitHub authorization lapsed; log in at hotpath.rs once (401).
    GithubAuthorizationExpired,
    /// Malformed query or path value (400).
    BadRequest,
    /// Unknown path or resource, including anything the caller may not see (404).
    NotFound,
    /// 405.
    MethodNotAllowed,
    /// 429; the `Retry-After` header says how many seconds to wait.
    RateLimited,
    /// 500.
    Internal,
    /// A code this client does not know; printed like any other error. Also
    /// the default, so a body without `code` still parses.
    #[default]
    #[serde(other)]
    Unknown,
}

/// Body of every non-2xx answer of `/api/v1`. The request id is the
/// `x-request-id` response header, not part of the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// What was wrong and what to do about it.
    pub error: String,
    #[serde(default)]
    pub code: ApiErrorCode,
}

/// Body of `GET /api/v1/auth`: the status of the credential sent, nothing
/// else (no ids, no email, no repositories).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthStatus {
    /// The GitHub login the token acts as.
    pub login: String,
    pub token: TokenStatus,
}

/// The personal API token behind an `AuthStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenStatus {
    /// The label given on creation.
    pub name: String,
    /// RFC 3339, UTC.
    pub expires_at: String,
}

/// What happened to the pull request comment, inside the 201 body. `url` set
/// means posted or updated; `error` set means it failed and says why; neither
/// means there was nothing to post (a push upload, for instance).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Body of a successful upload (201, or 200 when the server already had the
/// same run and returned the existing report).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadCreated {
    pub id: String,
    pub repository: String,
    pub benchmark: String,
    /// Report the upload is compared against (pull request uploads only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    /// Defaulted so a server that does not send it yet still parses.
    #[serde(default)]
    pub comment: CommentOutcome,
}

#[cfg(test)]
mod tests {
    use crate::json::cloud_api::{
        normalize_base_url, ApiError, ApiErrorCode, AuthStatus, CommentOutcome, TokenStatus,
        UploadCreated, DEFAULT_BASE_URL,
    };

    #[test]
    fn normalize_base_url_rules() {
        assert_eq!(normalize_base_url(None), DEFAULT_BASE_URL);
        assert_eq!(normalize_base_url(Some("   ".into())), DEFAULT_BASE_URL);
        assert_eq!(
            normalize_base_url(Some(" http://localhost:3000/// ".into())),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_base_url(Some("https://staging.hotpath.rs".into())),
            "https://staging.hotpath.rs"
        );
    }

    #[test]
    fn api_error_parses_known_unknown_and_missing_codes() {
        let err: ApiError =
            serde_json::from_str(r#"{"error":"nope","code":"invalid_token","later":1}"#).unwrap();
        assert_eq!(err.error, "nope");
        assert_eq!(err.code, ApiErrorCode::InvalidToken);

        let newer: ApiError =
            serde_json::from_str(r#"{"error":"nope","code":"quota_exceeded"}"#).unwrap();
        assert_eq!(newer.code, ApiErrorCode::Unknown);

        let bare: ApiError = serde_json::from_str(r#"{"error":"boom"}"#).unwrap();
        assert_eq!(bare.code, ApiErrorCode::Unknown);

        assert_eq!(
            serde_json::to_string(&ApiError {
                error: "Benchmark ci not found.".into(),
                code: ApiErrorCode::NotFound,
            })
            .unwrap(),
            r#"{"error":"Benchmark ci not found.","code":"not_found"}"#
        );
    }

    #[test]
    fn auth_status_round_trips() {
        let body =
            r#"{"login":"pawurb","token":{"name":"laptop","expires_at":"2027-01-01T00:00:00Z"}}"#;
        let status: AuthStatus = serde_json::from_str(body).unwrap();
        assert_eq!(
            status,
            AuthStatus {
                login: "pawurb".into(),
                token: TokenStatus {
                    name: "laptop".into(),
                    expires_at: "2027-01-01T00:00:00Z".into(),
                },
            }
        );
        assert_eq!(serde_json::to_string(&status).unwrap(), body);
    }

    #[test]
    fn upload_created_parses_with_and_without_comment() {
        let full: UploadCreated = serde_json::from_str(
            r#"{"id":"r1","repository":"a/b","benchmark":"meta","baseline":"r0",
                "comment":{"url":"https://github.com/c/1","error":"the report does not parse"},
                "later":true}"#,
        )
        .unwrap();
        assert_eq!(full.baseline.as_deref(), Some("r0"));
        assert_eq!(full.comment.url.as_deref(), Some("https://github.com/c/1"));
        assert_eq!(
            full.comment.error.as_deref(),
            Some("the report does not parse")
        );

        let bare: UploadCreated =
            serde_json::from_str(r#"{"id":"r1","repository":"a/b","benchmark":"meta"}"#).unwrap();
        assert_eq!(bare.baseline, None);
        assert_eq!(bare.comment, CommentOutcome::default());
    }

    #[test]
    fn optional_fields_are_omitted_when_none() {
        let json = serde_json::to_string(&UploadCreated {
            id: "r1".into(),
            repository: "a/b".into(),
            benchmark: "meta".into(),
            baseline: None,
            comment: CommentOutcome::default(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"id":"r1","repository":"a/b","benchmark":"meta","comment":{}}"#
        );
    }
}
