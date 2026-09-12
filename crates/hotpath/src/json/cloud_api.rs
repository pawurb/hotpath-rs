//! Wire types of the hotpath.rs upload API (`POST /api/v1/reports`), shared
//! by the `hotpath-cloud` client (`lib_on/cloud.rs`) and the hotpath-backend
//! server, which depends on this crate with the `json` feature.
//!
//! Plain structs on purpose. `code` and `reason` are strings the client
//! renders and never branches on: the server owns both the message and the
//! vocabulary, so a new code or reason prints as text and a new field is
//! ignored, and nothing can fail to deserialize because the server learned a
//! new word. The catalogue of codes and reasons is documented with the server.
//! The one decision the client makes from this data is "is there a `hint` to
//! show", and the server makes that choice by setting or omitting the field.

use serde::{Deserialize, Serialize};

/// Body of every non-2xx answer from `POST /api/v1/reports`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadError {
    /// Stable snake_case code (`event_mismatch`, `app_not_installed`, ...).
    pub code: String,
    /// Sentence for humans.
    pub error: String,
    /// Next step, ready to print.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Id of the server-side request span, for matching a CI line to a log
    /// line. Also sent as the `x-request-id` response header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// What happened to the pull request comment, inside the 201 body.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentOutcome {
    /// Set when a comment was posted or updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Why there is no comment, or a caveat on one that exists. Stable
    /// snake_case; for server logs, tests and the relay's step summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Set only when the server wants CI to show a warning. Absent means there
    /// is nothing for the adopter to act on (a push upload has no comment to
    /// post, for instance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
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
    use crate::json::cloud_api::{CommentOutcome, UploadCreated, UploadError};

    #[test]
    fn upload_error_parses_unknown_code_and_extra_fields() {
        let err: UploadError = serde_json::from_str(
            r#"{"code":"something_new","error":"nope","hint":"do this","request_id":"1bac4db9-15a","docs":"https://x"}"#,
        )
        .unwrap();
        assert_eq!(err.code, "something_new");
        assert_eq!(err.error, "nope");
        assert_eq!(err.hint.as_deref(), Some("do this"));
        assert_eq!(err.request_id.as_deref(), Some("1bac4db9-15a"));

        let minimal: UploadError =
            serde_json::from_str(r#"{"code":"server_error","error":"boom"}"#).unwrap();
        assert_eq!(minimal.hint, None);
        assert_eq!(minimal.request_id, None);
    }

    #[test]
    fn upload_created_parses_with_and_without_comment() {
        let full: UploadCreated = serde_json::from_str(
            r#"{"id":"r1","repository":"a/b","benchmark":"meta","baseline":"r0",
                "comment":{"url":"https://github.com/c/1","reason":"report_unreadable","hint":"fix it"},
                "later":true}"#,
        )
        .unwrap();
        assert_eq!(full.baseline.as_deref(), Some("r0"));
        assert_eq!(full.comment.url.as_deref(), Some("https://github.com/c/1"));
        assert_eq!(full.comment.reason.as_deref(), Some("report_unreadable"));
        assert_eq!(full.comment.hint.as_deref(), Some("fix it"));

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
