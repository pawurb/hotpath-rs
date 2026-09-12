//! Wire types of the hotpath.rs upload API (`POST /api/v1/reports`), shared
//! by the `hotpath-cloud-meta` client (`lib_on/cloud.rs`) and the hotpath-backend
//! server, which depends on this crate with the `json` feature.
//!
//! Deliberately minimal: the upload succeeded or failed, the comment was
//! posted or failed, and one sentence says why. The server owns that
//! sentence, the client prints it and branches on nothing in it.

use serde::{Deserialize, Serialize};

/// Body of every non-2xx answer from `POST /api/v1/reports`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadError {
    /// What was wrong and what to do about it.
    pub error: String,
    /// Id of the server-side request span, for matching a CI line to a log
    /// line. Also sent as the `x-request-id` response header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
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
    use crate::json::cloud_api::{CommentOutcome, UploadCreated, UploadError};

    #[test]
    fn upload_error_parses_with_extra_fields() {
        let err: UploadError =
            serde_json::from_str(r#"{"error":"nope","request_id":"1bac4db9-15a","code":"later"}"#)
                .unwrap();
        assert_eq!(err.error, "nope");
        assert_eq!(err.request_id.as_deref(), Some("1bac4db9-15a"));

        let minimal: UploadError = serde_json::from_str(r#"{"error":"boom"}"#).unwrap();
        assert_eq!(minimal.request_id, None);
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
