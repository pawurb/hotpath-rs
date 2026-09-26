//! Wire types of the hotpath.rs API, shared by the `hotpath-cloud` upload
//! client (`lib_on/cloud.rs`), the `hotpath cloud` CLI (`cloud` binary
//! feature) and the hotpath-backend server, which depends on this crate with
//! the `json` feature. Every body the server sends is defined here first;
//! the server serializes these types and keeps no structs of its own.
//!
//! Deliberately minimal: the server owns every sentence, the clients print it
//! and branch only on `ApiError::code`.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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
    /// Malformed query or path value, or an oversized upload body (400, 413).
    BadRequest,
    /// The token's user may not act on this resource, for instance an upload
    /// to a repository the GitHub App is not installed on (403).
    Forbidden,
    /// Unknown path or resource, including anything the caller may not see (404).
    NotFound,
    MethodNotAllowed,
    /// 429; the `Retry-After` header says how many seconds to wait.
    RateLimited,
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
    /// RFC 3339 on the wire.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// Body of `GET /api/v1/repos`: every active repository the token's user can
/// see on GitHub with the hotpath App installed, ordered by `full_name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoList {
    pub repositories: Vec<Repository>,
}

/// One repository of a `RepoList`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// `owner/name` as GitHub names it today (renames follow GitHub by id).
    pub full_name: String,
    /// GitHub's visibility.
    pub private: bool,
    /// The dashboard's own choice to show this repository's reports to
    /// everyone (`repos.visibility_public`); never true on a private repo.
    /// Informational for now: it gates nothing yet.
    pub visibility_public: bool,
    /// The repository's benchmarks, ordered by name; empty until the first upload.
    pub benchmarks: Vec<BenchmarkSummary>,
}

/// One benchmark of a repository, as both `repos` and `benchmarks` list it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub name: String,
    /// Stored reports.
    pub reports: u64,
    /// Upload time of the newest report; `None` (`null` on the wire, never
    /// omitted) for a benchmark with none yet (created by a rejected first
    /// upload, or still running).
    #[serde(with = "time::serde::rfc3339::option")]
    pub latest_report_at: Option<OffsetDateTime>,
}

/// Body of `GET /api/v1/repos/{owner}/{name}/benchmarks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkList {
    /// The repository's current `full_name`, which may differ from the path
    /// when the caller used a name GitHub has since renamed.
    pub repository: String,
    /// Ordered by name.
    pub benchmarks: Vec<BenchmarkSummary>,
}

/// `owner/name` from a git remote URL on github.com, in the SSH
/// (`git@github.com:o/n.git`), HTTPS (`https://github.com/o/n(.git)`) and
/// `ssh://git@github.com/o/n(.git)` forms; `None` for any other host, a
/// filesystem remote or a path that is not exactly two segments. The one
/// remote URL parser: the upload's `meta.git.repository` (`lib_on/git_info.rs`)
/// and the `hotpath cloud` CLI's `--repo` fallback both use it. Characters
/// are not validated here; the CLI checks them before building a path.
pub fn repository_from_remote_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?,
        // scp-like `[user@]host:path`.
        None => url.split_once(':')?,
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split_once(':').map_or(host, |(host, _port)| host);
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let mut segments = path.split('/');
    let owner = segments.next().filter(|s| !s.is_empty())?;
    let name = segments.next().filter(|s| !s.is_empty())?;
    if segments.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{name}"))
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
        normalize_base_url, repository_from_remote_url, ApiError, ApiErrorCode, AuthStatus,
        BenchmarkList, BenchmarkSummary, CommentOutcome, RepoList, Repository, TokenStatus,
        UploadCreated, DEFAULT_BASE_URL,
    };
    use time::macros::datetime;

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
                    expires_at: datetime!(2027-01-01 00:00:00 UTC),
                },
            }
        );
        assert_eq!(serde_json::to_string(&status).unwrap(), body);

        assert!(serde_json::from_str::<AuthStatus>(
            r#"{"login":"pawurb","token":{"name":"laptop","expires_at":"tomorrow"}}"#,
        )
        .is_err());
    }

    #[test]
    fn repo_list_round_trips() {
        let body = r#"{"repositories":[{"full_name":"pawurb/hotpath-rs","private":false,"visibility_public":true,"benchmarks":[{"name":"ci","reports":412,"latest_report_at":"2026-09-25T18:03:11Z"},{"name":"empty","reports":0,"latest_report_at":null}]},{"full_name":"pawurb/private-thing","private":true,"visibility_public":false,"benchmarks":[]}]}"#;
        let list: RepoList = serde_json::from_str(body).unwrap();
        assert_eq!(
            list,
            RepoList {
                repositories: vec![
                    Repository {
                        full_name: "pawurb/hotpath-rs".into(),
                        private: false,
                        visibility_public: true,
                        benchmarks: vec![
                            BenchmarkSummary {
                                name: "ci".into(),
                                reports: 412,
                                latest_report_at: Some(datetime!(2026-09-25 18:03:11 UTC)),
                            },
                            BenchmarkSummary {
                                name: "empty".into(),
                                reports: 0,
                                latest_report_at: None,
                            },
                        ],
                    },
                    Repository {
                        full_name: "pawurb/private-thing".into(),
                        private: true,
                        visibility_public: false,
                        benchmarks: vec![],
                    },
                ],
            }
        );
        assert_eq!(serde_json::to_string(&list).unwrap(), body);

        // `latest_report_at` is nullable, not optional.
        assert!(serde_json::from_str::<BenchmarkSummary>(r#"{"name":"ci","reports":1}"#).is_err());
    }

    #[test]
    fn benchmark_list_round_trips() {
        let body = r#"{"repository":"pawurb/hotpath-rs","benchmarks":[{"name":"ci","reports":412,"latest_report_at":"2026-09-25T18:03:11Z"}]}"#;
        let list: BenchmarkList = serde_json::from_str(body).unwrap();
        assert_eq!(
            list,
            BenchmarkList {
                repository: "pawurb/hotpath-rs".into(),
                benchmarks: vec![BenchmarkSummary {
                    name: "ci".into(),
                    reports: 412,
                    latest_report_at: Some(datetime!(2026-09-25 18:03:11 UTC)),
                }],
            }
        );
        assert_eq!(serde_json::to_string(&list).unwrap(), body);
    }

    #[test]
    fn repository_from_remote_url_forms() {
        for url in [
            "git@github.com:pawurb/hotpath-rs.git",
            "git@github.com:pawurb/hotpath-rs",
            "https://github.com/pawurb/hotpath-rs.git",
            "https://github.com/pawurb/hotpath-rs",
            "https://github.com/pawurb/hotpath-rs/",
            "https://user:token@github.com/pawurb/hotpath-rs.git",
            "ssh://git@github.com/pawurb/hotpath-rs.git",
            "ssh://git@github.com/pawurb/hotpath-rs",
            "ssh://git@github.com:22/pawurb/hotpath-rs.git",
            "git@GitHub.com:pawurb/hotpath-rs.git",
            "  git@github.com:pawurb/hotpath-rs.git\n",
        ] {
            assert_eq!(
                repository_from_remote_url(url).as_deref(),
                Some("pawurb/hotpath-rs"),
                "{url}"
            );
        }
        for url in [
            "hotpath-rs",
            "",
            "/srv/repos/hotpath-rs",
            "../repos/hotpath-rs",
            "file:///srv/repos/hotpath-rs",
            "https://github.com/pawurb",
            "https://github.com/pawurb/hotpath-rs/extra",
            "git@gitlab.com:pawurb/hotpath-rs.git",
            "https://gitlab.com/pawurb/hotpath-rs.git",
            "ssh://git@bitbucket.org/pawurb/hotpath-rs.git",
            "https://github.com.evil.io/pawurb/hotpath-rs.git",
            "https://notgithub.com/pawurb/hotpath-rs.git",
        ] {
            assert_eq!(repository_from_remote_url(url), None, "{url}");
        }
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
