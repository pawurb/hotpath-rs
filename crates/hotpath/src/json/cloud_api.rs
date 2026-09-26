//! Wire types of the hotpath.rs API, shared by the `hotpath-cloud` upload
//! client (`lib_on/cloud.rs`), the `hotpath cloud` CLI (`cloud` binary
//! feature) and the hotpath-backend server, which depends on this crate with
//! the `json` feature. Every body the server sends is defined here first;
//! the server serializes these types and keeps no structs of its own.
//!
//! Deliberately minimal: the server owns every sentence, the clients print it
//! and branch only on `ApiError::code`.

use std::collections::BTreeMap;
use std::fmt;

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

/// A benchmark name that breaks the rule every party enforces: 1-64 chars of
/// `[A-Za-z0-9._-]`, not `.` or `..`. Displays as the rule itself, so each
/// caller prefixes where the name came from (`HOTPATH_BENCHMARK`,
/// `--benchmark`). The server's copy (hotpath-backend
/// `models::benchmark::validate_name`) must stay byte for byte identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidBenchmarkName;

impl fmt::Display for InvalidBenchmarkName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("use 1-64 chars from [A-Za-z0-9._-], not \".\" or \"..\"")
    }
}

/// Checks a benchmark name before it names an upload series or goes into a
/// request path; a valid name needs no percent-encoding.
pub fn validate_benchmark_name(name: &str) -> Result<(), InvalidBenchmarkName> {
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    if name.is_empty() || name.len() > 64 || !valid_chars || name == "." || name == ".." {
        return Err(InvalidBenchmarkName);
    }
    Ok(())
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
    /// A submitted PR comment policy was refused (422); the body is a
    /// `PolicyRejected` with every problem found.
    InvalidPolicy,
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

/// One stored report without its payload: the body of `reports/latest` and
/// `reports/{id}` with `payload=false`, and the head of a `Report`. Every
/// nullable field serializes as `null`, never omitted, so a reader sees
/// "unknown" rather than a missing key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSummary {
    /// The report's id (a uuid), the value `--id` and the dashboard URLs take.
    pub id: String,
    /// `owner/name` as GitHub names it today.
    pub repository: String,
    /// The benchmark (series) the report belongs to.
    pub benchmark: String,
    /// `push`, `pull_request`, or whatever event the run reported.
    pub event: String,
    /// The commit that was checked out and measured. For a pull request run
    /// this is usually GitHub's merge commit, not the PR head: see `head_sha`.
    pub commit_sha: String,
    /// The pull request's head commit, the one a developer has locally;
    /// `None` for push reports and when the client could not read it.
    pub head_sha: Option<String>,
    /// The base branch commit the pull request was measured against.
    pub base_sha: Option<String>,
    /// The measured ref (`refs/heads/main`); `None` on a detached checkout,
    /// which is every default pull request checkout.
    pub git_ref: Option<String>,
    /// Bare base branch name of a pull request (`main`).
    pub base_ref: Option<String>,
    /// Bare head branch name of a pull request (`feature-x`).
    pub head_ref: Option<String>,
    /// The pull request number; `None` for push reports.
    pub pr_number: Option<u64>,
    /// The CI run id. Text, not a number: only GitHub's run ids happen to be
    /// numeric.
    pub run_id: Option<String>,
    /// The CI workflow name.
    pub workflow: Option<String>,
    /// The login that triggered the run.
    pub actor: Option<String>,
    /// `github-actions` today.
    pub ci_provider: Option<String>,
    /// The hotpath version that wrote the report.
    pub hotpath_version: Option<String>,
    /// `HOTPATH_USER_METADATA` pairs attached by the profiled program, sorted.
    pub user_metadata: Option<BTreeMap<String, String>>,
    /// The report this one was compared against in its PR comment (a push
    /// report on the base branch); `None` when there was none.
    pub baseline_id: Option<String>,
    /// The PR comment this report produced, when it was posted.
    pub comment_url: Option<String>,
    /// Size of the uploaded JSON in bytes.
    pub size_bytes: u64,
    /// Upload time, RFC 3339 on the wire.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// The report's page on the dashboard.
    pub dashboard_url: String,
}

/// A report with its payload: the body of `reports/latest` and `reports/{id}`
/// by default. Which of `Report` and `ReportSummary` a response is follows
/// from the request (`payload=false` or not), never from the body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    #[serde(flatten)]
    pub summary: ReportSummary,
    /// The uploaded hotpath JSON report. Untyped on purpose: a report older
    /// than the schema the server reads must still be fetchable (only a diff
    /// calls it unreadable). Deserialize it as `JsonReport` when a typed view
    /// is needed. Re-serialized through `serde_json::Value`, so object keys
    /// come back sorted; the data is unchanged.
    pub payload: serde_json::Value,
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

/// Largest PR comment policy document the server stores, in bytes of UTF-8.
pub const POLICY_MAX_BYTES: usize = 65536;

/// Which stored document a policy view comes from. A benchmark policy
/// overrides the repo policy for that benchmark; levels do not inherit from
/// each other: the one that applies is the benchmark's if stored, else the
/// repo's if stored, else the built-in default, and any key a stored document
/// omits takes the built-in value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyLevel {
    /// Nothing is stored at either level: the built-in default applies.
    Default,
    /// The repository's document, which applies to every benchmark without
    /// one of its own.
    Repo,
    /// One benchmark's document.
    Benchmark,
}

/// Body of `GET /api/v1/repos/{owner}/{name}/policy` and
/// `GET .../benchmarks/{benchmark}/policy`: the PR comment policy in force for
/// the scope asked about. Reading needs only access to the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyView {
    /// `owner/name` as GitHub names it today.
    pub repository: String,
    /// The benchmark asked about; `None` for the repo level.
    pub benchmark: Option<String>,
    /// Which level's document `source` is. For a benchmark it can be `Repo` or
    /// `Default` (nothing stored for the benchmark); for the repo level it is
    /// `Repo` or `Default`.
    pub level: PolicyLevel,
    /// Whether a document is stored at the level asked about. `false` means
    /// the scope inherits and `source` is what it inherits, a starting point
    /// for an edit.
    pub stored: bool,
    /// The TOML document, as written by whoever saved it (or the built-in
    /// default, verbatim).
    pub source: String,
    /// Set when the stored document no longer parses under the server's
    /// current rules and the built-in default judges instead: the sentence
    /// saying so. `None` when `source` is what judges.
    pub fallback: Option<String>,
}

/// Body of `PUT /api/v1/repos/{owner}/{name}/policy` and
/// `PUT .../benchmarks/{benchmark}/policy`. Writing needs push permission on
/// the repository (checked with GitHub per request, `403 forbidden` without
/// it), and a benchmark must exist (created by its first upload) to hold a
/// policy (`404 not_found` otherwise).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyUpdate {
    /// The whole TOML document to store at the scope the path names, the same
    /// text the dashboard's policy editor saves, stored as written. It
    /// replaces what is stored there; nothing is merged. At most
    /// `POLICY_MAX_BYTES`; blank is refused.
    pub source: String,
    /// Validate only: the server checks the document and answers as it would,
    /// but stores nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Body of a successful `PUT .../policy` (200).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySaved {
    /// `owner/name` as GitHub names it today.
    pub repository: String,
    /// The benchmark written; `None` for the repo level.
    pub benchmark: Option<String>,
    /// `Repo` or `Benchmark`: the level written (or that would be, on a dry run).
    pub level: PolicyLevel,
    /// `true` when nothing was stored because the request asked only to validate.
    pub dry_run: bool,
}

/// One thing wrong with a submitted policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyProblem {
    /// 1-based line of the submitted text the problem points at; `None` when
    /// it concerns the document as a whole (size, blank) or no line can be
    /// named.
    pub line: Option<u32>,
    /// What is wrong and, where the server can say, what is allowed
    /// (`functions.alloc.min_percent_change must be between 0 and 1000, got 5000`).
    pub message: String,
}

/// Body of `422` from `PUT .../policy` when the document is refused: an
/// `ApiError` (`error`, `code` = `invalid_policy`) plus every problem found. A
/// client that only knows `ApiError` still parses it (unknown fields are
/// ignored); one that knows this type reads `problems`.
///
/// The server collects all the problems it can in one pass, within what TOML
/// allows. A syntax error (an unbalanced quote, a bad table header) stops
/// parsing, so it is the only problem reported: nothing after it can be read
/// reliably. Once the text parses, structural problems (an unknown key, a
/// wrong value type, a column name the resource does not have) and range
/// problems (a percent outside its bounds, an empty `metrics` list, a column
/// named twice, too many `ignore` patterns) are all reported together, each
/// with its line when it can be found. So `problems` has one item for a
/// syntax error and every item otherwise; never assume a single item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRejected {
    /// One sentence summing up (`The policy has 3 problems.`).
    pub error: String,
    /// `InvalidPolicy`.
    pub code: ApiErrorCode,
    /// Never empty. Ordered by line, whole-document problems first.
    pub problems: Vec<PolicyProblem>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::json::cloud_api::{
        normalize_base_url, validate_benchmark_name, ApiError, ApiErrorCode, AuthStatus,
        BenchmarkList, BenchmarkSummary, CommentOutcome, PolicyLevel, PolicyProblem,
        PolicyRejected, PolicySaved, PolicyUpdate, PolicyView, RepoList, Report, ReportSummary,
        Repository, TokenStatus, UploadCreated, DEFAULT_BASE_URL,
    };
    use time::macros::datetime;

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

        let err = validate_benchmark_name("a/b").unwrap_err().to_string();
        assert!(
            err.contains("[A-Za-z0-9._-]"),
            "message names the rule: {err}"
        );
    }

    const PR_SUMMARY: &str = r#"{"id":"0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a","repository":"pawurb/hotpath-rs","benchmark":"ci","event":"pull_request","commit_sha":"3f1c000000000000000000000000000000000000","head_sha":"9ab2000000000000000000000000000000000000","base_sha":"77de000000000000000000000000000000000000","git_ref":null,"base_ref":"main","head_ref":"channel-delay","pr_number":105,"run_id":"18237461234","workflow":"CI","actor":"pawurb","ci_provider":"github-actions","hotpath_version":"0.26.1","user_metadata":{"profile":"release"},"baseline_id":"0199a3b0-0000-7000-8000-000000000000","comment_url":"https://github.com/pawurb/hotpath-rs/pull/105#issuecomment-1","size_bytes":81234,"created_at":"2026-09-25T18:03:11Z","dashboard_url":"https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a"}"#;
    const PUSH_SUMMARY: &str = r#"{"id":"0199a3b0-0000-7000-8000-000000000000","repository":"pawurb/hotpath-rs","benchmark":"ci","event":"push","commit_sha":"77de000000000000000000000000000000000000","head_sha":null,"base_sha":null,"git_ref":"refs/heads/main","base_ref":null,"head_ref":null,"pr_number":null,"run_id":"18237400000","workflow":"CI","actor":"pawurb","ci_provider":"github-actions","hotpath_version":"0.26.1","user_metadata":null,"baseline_id":null,"comment_url":null,"size_bytes":80000,"created_at":"2026-09-25T17:00:00Z","dashboard_url":"https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3b0-0000-7000-8000-000000000000"}"#;

    fn pr_summary() -> ReportSummary {
        ReportSummary {
            id: "0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a".into(),
            repository: "pawurb/hotpath-rs".into(),
            benchmark: "ci".into(),
            event: "pull_request".into(),
            commit_sha: "3f1c000000000000000000000000000000000000".into(),
            head_sha: Some("9ab2000000000000000000000000000000000000".into()),
            base_sha: Some("77de000000000000000000000000000000000000".into()),
            git_ref: None,
            base_ref: Some("main".into()),
            head_ref: Some("channel-delay".into()),
            pr_number: Some(105),
            run_id: Some("18237461234".into()),
            workflow: Some("CI".into()),
            actor: Some("pawurb".into()),
            ci_provider: Some("github-actions".into()),
            hotpath_version: Some("0.26.1".into()),
            user_metadata: Some(BTreeMap::from([("profile".into(), "release".into())])),
            baseline_id: Some("0199a3b0-0000-7000-8000-000000000000".into()),
            comment_url: Some(
                "https://github.com/pawurb/hotpath-rs/pull/105#issuecomment-1".into(),
            ),
            size_bytes: 81234,
            created_at: datetime!(2026-09-25 18:03:11 UTC),
            dashboard_url: "https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a".into(),
        }
    }

    #[test]
    fn report_summary_round_trips_with_nulls_present() {
        let summary: ReportSummary = serde_json::from_str(PR_SUMMARY).unwrap();
        assert_eq!(summary, pr_summary());
        assert_eq!(serde_json::to_string(&summary).unwrap(), PR_SUMMARY);

        let push: ReportSummary = serde_json::from_str(PUSH_SUMMARY).unwrap();
        assert_eq!(push.head_sha, None);
        assert_eq!(push.pr_number, None);
        assert_eq!(push.user_metadata, None);
        assert_eq!(push.git_ref.as_deref(), Some("refs/heads/main"));
        assert_eq!(serde_json::to_string(&push).unwrap(), PUSH_SUMMARY);
    }

    #[test]
    fn report_flattens_the_summary_and_keeps_the_payload_last() {
        // Payload keys already sorted: `Value` re-serializes objects in key
        // order, so only a sorted payload round-trips byte for byte.
        let payload = r#"{"meta":{},"version":"0.26.1"}"#;
        let body = format!(
            "{},\"payload\":{payload}}}",
            &PR_SUMMARY[..PR_SUMMARY.len() - 1]
        );
        let report: Report = serde_json::from_str(&body).unwrap();
        assert_eq!(
            report,
            Report {
                summary: pr_summary(),
                payload: serde_json::from_str(payload).unwrap(),
            }
        );
        assert_eq!(serde_json::to_string(&report).unwrap(), body);

        // A payload in the writer's key order parses to the same data.
        let unsorted = body.replace(payload, r#"{"version":"0.26.1","meta":{}}"#);
        let reparsed: Report = serde_json::from_str(&unsorted).unwrap();
        assert_eq!(reparsed, report);
    }

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

    #[test]
    fn policy_view_round_trips() {
        let repo = r#"{"repository":"pawurb/hotpath-rs","benchmark":null,"level":"repo","stored":true,"source":"[functions.timing]\nmin_percent_change = 5\n","fallback":null}"#;
        let view: PolicyView = serde_json::from_str(repo).unwrap();
        assert_eq!(
            view,
            PolicyView {
                repository: "pawurb/hotpath-rs".into(),
                benchmark: None,
                level: PolicyLevel::Repo,
                stored: true,
                source: "[functions.timing]\nmin_percent_change = 5\n".into(),
                fallback: None,
            }
        );
        assert_eq!(serde_json::to_string(&view).unwrap(), repo);

        // A benchmark with nothing stored inherits the repo document.
        let inheriting = r##"{"repository":"pawurb/hotpath-rs","benchmark":"ci","level":"repo","stored":false,"source":"# repo\n","fallback":null}"##;
        let view: PolicyView = serde_json::from_str(inheriting).unwrap();
        assert_eq!(view.benchmark.as_deref(), Some("ci"));
        assert_eq!(view.level, PolicyLevel::Repo);
        assert!(!view.stored);
        assert_eq!(serde_json::to_string(&view).unwrap(), inheriting);

        let fallback = r#"{"repository":"pawurb/hotpath-rs","benchmark":"ci","level":"benchmark","stored":true,"source":"[old]\n","fallback":"The stored policy no longer parses; the built-in default applies."}"#;
        let view: PolicyView = serde_json::from_str(fallback).unwrap();
        assert_eq!(view.level, PolicyLevel::Benchmark);
        assert_eq!(
            view.fallback.as_deref(),
            Some("The stored policy no longer parses; the built-in default applies.")
        );
        assert_eq!(serde_json::to_string(&view).unwrap(), fallback);

        let default = r#"{"repository":"a/b","benchmark":null,"level":"default","stored":false,"source":"","fallback":null,"later":1}"#;
        let view: PolicyView = serde_json::from_str(default).unwrap();
        assert_eq!(view.level, PolicyLevel::Default);
    }

    #[test]
    fn policy_update_and_saved_round_trip() {
        let body = r#"{"source":"[functions]\n","dry_run":true}"#;
        let update: PolicyUpdate = serde_json::from_str(body).unwrap();
        assert_eq!(
            update,
            PolicyUpdate {
                source: "[functions]\n".into(),
                dry_run: true,
            }
        );
        assert_eq!(serde_json::to_string(&update).unwrap(), body);

        let omitted: PolicyUpdate = serde_json::from_str(r#"{"source":"x = 1"}"#).unwrap();
        assert!(!omitted.dry_run);

        let saved =
            r#"{"repository":"pawurb/hotpath-rs","benchmark":null,"level":"repo","dry_run":false}"#;
        let parsed: PolicySaved = serde_json::from_str(saved).unwrap();
        assert_eq!(
            parsed,
            PolicySaved {
                repository: "pawurb/hotpath-rs".into(),
                benchmark: None,
                level: PolicyLevel::Repo,
                dry_run: false,
            }
        );
        assert_eq!(serde_json::to_string(&parsed).unwrap(), saved);
    }

    #[test]
    fn policy_rejected_round_trips_and_parses_as_api_error() {
        let body = r#"{"error":"The policy has 3 problems.","code":"invalid_policy","problems":[{"line":null,"message":"the policy is larger than 65536 bytes"},{"line":3,"message":"unknown key `functions.timing.min_percent`"},{"line":7,"message":"functions.alloc.min_percent_change must be between 0 and 1000, got 5000"}]}"#;
        let rejected: PolicyRejected = serde_json::from_str(body).unwrap();
        assert_eq!(
            rejected,
            PolicyRejected {
                error: "The policy has 3 problems.".into(),
                code: ApiErrorCode::InvalidPolicy,
                problems: vec![
                    PolicyProblem {
                        line: None,
                        message: "the policy is larger than 65536 bytes".into(),
                    },
                    PolicyProblem {
                        line: Some(3),
                        message: "unknown key `functions.timing.min_percent`".into(),
                    },
                    PolicyProblem {
                        line: Some(7),
                        message:
                            "functions.alloc.min_percent_change must be between 0 and 1000, got 5000"
                                .into(),
                    },
                ],
            }
        );
        assert_eq!(serde_json::to_string(&rejected).unwrap(), body);

        let plain: ApiError = serde_json::from_str(body).unwrap();
        assert_eq!(plain.code, ApiErrorCode::InvalidPolicy);
        assert_eq!(plain.error, "The policy has 3 problems.");
    }
}
