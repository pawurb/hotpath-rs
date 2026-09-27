//! Wire types of the hotpath.rs API, shared by the `hotpath-cloud-meta` upload
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

use crate::json::JsonLocation;
use crate::output::ProfilingMode;

/// Base URL of the hotpath.rs API when nothing overrides it.
pub const DEFAULT_BASE_URL: &str = "https://hotpath.rs";

/// Turns the raw value of `HOTPATH_META_UPLOAD_URL` / `HOTPATH_META_API_URL` into a
/// base URL: trimmed, without trailing slashes, `DEFAULT_BASE_URL` when unset
/// or blank.
pub fn normalize_base_url(raw: Option<String>) -> String {
    raw.map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// A benchmark name that breaks the rule every party enforces: 1-64 chars of
/// `[A-Za-z0-9._-]`, not `.` or `..`. Displays as the rule itself, so each
/// caller prefixes where the name came from (`HOTPATH_META_BENCHMARK`,
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
    /// `HOTPATH_META_USER_METADATA` pairs attached by the profiled program, sorted.
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

/// Body of `GET /api/v1/repos/{owner}/{name}/benchmarks/{benchmark}/diff`:
/// one head report against the baseline recorded for it at upload, judged
/// under the benchmark's policy in force now (not the one at upload time, so
/// a `set-policy` followed by a diff shows the new judgement without a
/// re-run).
///
/// The head is picked like `reports/latest` / `reports/{id}` pick a report:
/// `pr=N` or `commit=SHA` (newest match by upload, narrowed by `event=`), or
/// `head=ID`. The baseline is always the head's recorded `baseline_id`, the
/// one the PR comment compared. A `head=` of another benchmark is
/// `404 not_found`. All three `DiffResult`s answer 200. Reading needs only
/// access to the repository. `rows=findings` (the default) or
/// `rows=all` picks which rows the sections carry (see `RowFilter`); an
/// unknown value is `400 bad_request`. The body carries only the sections of
/// the families the policy lists.
///
/// The policy's budgets are judged on head alone, so `verdict` and `budgets`
/// sit here and not inside `Comparison`: a head without a baseline is still
/// answered for.
///
/// Numbers stay numbers: no formatted strings anywhere, a value is a number
/// in its column's `unit` and a change is a number of percent. Formatting is
/// the reader's job. Names deliberately differ from the server's analyzer
/// types where those read badly on the wire (`Presence::Both` / `Added`,
/// `Change`, `DiffCell`); the server maps its types into these in one place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportDiff {
    /// `owner/name` as GitHub names it today.
    pub repository: String,
    /// The benchmark (series) both reports belong to.
    pub benchmark: String,
    /// The report the selector picked.
    pub head: ReportSummary,
    /// The report compared against; `None` exactly when `result` is
    /// `NoBaseline`.
    pub base: Option<DiffBase>,
    /// The policy that judged (for `Compared`) or would judge.
    pub policy: AppliedPolicy,
    /// Which rows `DiffSection::rows` carries: the `rows=` the server
    /// applied, so a filtered body never reads as a complete one.
    pub rows: RowFilter,
    pub verdict: Verdict,
    /// The budgets judged on head; `None` when head does not parse (nothing
    /// to read them from). Present, with `rules: 0`, under a policy without
    /// budgets.
    pub budgets: Option<Budgets>,
    pub result: DiffResult,
    /// The dashboard's comparison page for the head report
    /// (`.../benchmarks/{benchmark}/reports/{head_id}/diff`), which exists
    /// with or without a baseline.
    pub dashboard_url: String,
}

/// Which rows the sections of a `ReportDiff` carry, as `rows=` asks. Only
/// rows are filtered: the sections the policy lists, their columns, family
/// and `counts`, the verdict, totals, `skipped` and `notes` are the same under
/// both, so the counts still say how many rows each outcome had. The same
/// filter cuts `Budgets::findings` to the broken ones; `Budgets::rules`,
/// `broken` and `notes` are never cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowFilter {
    /// The default: the rows the PR comment lists, `Regression`,
    /// `Improvement`, `Added` and `Removed` rows of judged families. Every
    /// row of an unjudged family and the unchanged, below-floor, ignored and
    /// too-few-calls rows of a judged one are left out; `counts` still count
    /// them and `All` lists them.
    Findings,
    /// Every row of every section sent (a section the policy leaves out is
    /// never sent, under either filter).
    All,
    /// A filter this client does not know.
    #[serde(other)]
    Unknown,
}

/// The baseline side of a `ReportDiff`: the baseline recorded for head at
/// upload, the one the PR comment compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffBase {
    pub report: ReportSummary,
    /// Whether the baseline measured the commit head branched from
    /// (`head.base_sha`). `false`: another report stood in (the base
    /// branch's newest), so the base branch's own drift since the branch
    /// point is in the diff.
    pub branch_point: bool,
}

/// Which policy judged a `ReportDiff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedPolicy {
    /// Which level's document judged (see `PolicyLevel`).
    pub level: PolicyLevel,
    /// Set when the stored document no longer parses and the built-in
    /// default judged instead: the sentence saying so (as
    /// `PolicyView::fallback`).
    pub fallback: Option<String>,
}

/// The outcome of comparing head with its baseline, internally tagged:
/// `{"status": "compared", ...}`. The answer for the report as a whole,
/// budgets included, is `ReportDiff::verdict`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DiffResult {
    Compared(Comparison),
    /// Head has no recorded baseline (a push report, a PR whose base branch
    /// had no report yet).
    NoBaseline,
    /// A side does not parse under the server's report schema: what the PR
    /// comment says in that case, structured.
    Unreadable {
        side: DiffSide,
        /// The side's `hotpath_version`, when it reported one.
        hotpath_version: Option<String>,
        /// The parser's message.
        error: String,
    },
}

/// One side of a `ReportDiff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Base,
    Head,
}

/// A judged comparison of two reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// Run-level numbers; each `None` when either side lacks it.
    pub totals: RunTotals,
    /// Every section both reports carry whose family the policy lists, in
    /// display order.
    pub sections: Vec<DiffSection>,
    /// Sections both reports carry that could not be compared (profiling-mode
    /// or percentile-set mismatch), one sentence each. A section the policy
    /// leaves out is never named here either.
    pub skipped: Vec<String>,
    /// What the policy could not judge, such as a named percentile the report
    /// lacks, one sentence each.
    pub notes: Vec<String>,
}

/// The answer for the head report: the judged families of the diff (when
/// there is one) and the budgets, together. The CLI's exit code reads it: 0
/// for `judged && !regressed`, 1 for everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// Whether anything was judged: the diff was compared, or the policy has
    /// budget rules and head could be read. `false` for a head that does not
    /// parse, and for a report with no comparable baseline under a policy
    /// without budgets.
    pub judged: bool,
    /// A judged family regressed or a budget is broken:
    /// `regressions > 0 || budgets_broken > 0`.
    pub regressed: bool,
    /// Regressions of judged families; 0 when nothing was compared.
    pub regressions: u64,
    /// Improvements of judged families; 0 when nothing was compared.
    pub improvements: u64,
    /// Broken budget checks (`Budgets::broken`); 0 when head was not read.
    pub budgets_broken: u64,
}

/// The policy's budgets, judged on the head report alone. A budget is a rule
/// of the policy (`[[functions.budgets]]`, `[[sql.budgets]]`, ... one array
/// per resource): an absolute bound on a named entity, such as
/// `alloc = { avg = "1 KB" }` or `calls = { min = 10, max = 1000 }`. No
/// baseline is needed, and the family settings (`judged`, `ignore`, the floor,
/// `min_calls`) do not apply: a rule names its entity, so it is always
/// checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Budgets {
    /// Rules the policy has, across resources. A rule that holds may leave
    /// no finding, so "does the policy have budgets" is read here.
    pub rules: u64,
    /// Broken checks, counted before `rows` cuts `findings`.
    pub broken: u64,
    /// One per check: per resource in policy order, per rule in document
    /// order, the count checks of a rule first. Under `RowFilter::Findings`
    /// only the broken ones, as the PR comment lists them; every check under
    /// `All`.
    pub findings: Vec<BudgetFinding>,
    /// What could not be checked, one sentence each: an entity that may sit
    /// below the cut of a truncated report, a byte bound on a report profiled
    /// by allocation count, a percentile the report lacks. Not cut by `rows`.
    pub notes: Vec<String>,
}

/// One bound of one rule, checked against one entity. Every rule also
/// asserts its entity ran (an implied minimum of 1 on the count, unless the
/// rule writes `min = 0`); that implied check yields a finding only when it
/// is broken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetFinding {
    pub resource: DiffResource,
    /// The rule's position in the resource's `budgets` array, 0-based.
    pub rule: u32,
    /// The rule's `match`, as written (`*` matches any run of characters).
    pub pattern: String,
    /// The rule's `message`.
    pub message: Option<String>,
    /// The entity checked; `None` when nothing in the report matched the
    /// rule (the implied minimum is then the broken check).
    pub entity: Option<BudgetEntity>,
    pub check: BudgetCheck,
    /// Which side of the bound is allowed.
    pub bound: BoundKind,
    /// The unit of `limit` and `actual`.
    pub unit: Unit,
    /// The bound, in `unit`.
    pub limit: f64,
    /// Head's value, in `unit`; 0 for a count when `entity` is `None`.
    pub actual: f64,
    pub broken: bool,
}

/// The entity a `BudgetFinding` checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetEntity {
    /// What the rule's pattern matched against.
    pub key: String,
    /// What a reader sees.
    pub name: String,
    pub location: Option<JsonLocation>,
}

/// What a finding checked, internally tagged: `{"on": "count", ...}`. There
/// is no `Unknown` variant (serde's `other` does not work on an internally
/// tagged enum with data): a check kind this client does not know fails the
/// parse, the right answer for a verdict it could not read in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "on", rename_all = "snake_case")]
pub enum BudgetCheck {
    /// The resource's count (`calls`, `count`, `sent_count`).
    Count {
        /// The key as the rule writes it.
        name: String,
        /// The minimum of 1 every rule implies, not one the rule wrote.
        implied: bool,
    },
    /// A bound a family table of the rule wrote (`alloc = { avg = ... }`).
    Column {
        family: FamilyName,
        /// The section the column is read from.
        kind: SectionKind,
        /// The column's policy name (`avg`, `p99.9`, `wait_p95`).
        column: String,
    },
}

/// Which side of a budget's bound is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundKind {
    /// `actual` must not exceed `limit`.
    Max,
    /// `actual` must not fall below `limit`.
    Min,
    /// A bound this client does not know.
    #[serde(other)]
    Unknown,
}

/// Run-level numbers of a `Comparison`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RunTotals {
    /// Wall time of the run, nanoseconds.
    pub elapsed: Option<Change>,
    /// Bytes allocated over the run (alloc-bytes mode only).
    pub allocated: Option<Change>,
    /// Peak RSS of the process, bytes.
    pub peak_rss: Option<Change>,
}

/// Both sides of one number and the relative change. Values are in the
/// natural integer scale of their unit carried as `f64` (every real value is
/// far below 2^53, so nothing is lost).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub base: f64,
    pub head: f64,
    /// `(head - base) / base * 100`; `0 -> x` is `100`, `0 -> 0` is `0`.
    pub change_percent: f64,
}

/// One section of a `Comparison`: a resource's table in one kind (the
/// `functions` alloc table, the `sql` table, ...). The sibling section of the
/// same resource (timing next to alloc) is in the same body and matches by
/// `DiffRow::key`; rows carry no copy of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffSection {
    pub resource: DiffResource,
    pub kind: SectionKind,
    /// The profiling mode, for `functions` and `server` sections; the wire
    /// form is the report's own (`timing`, `alloc-bytes`, `alloc-count`).
    pub mode: Option<ProfilingMode>,
    /// The section's own totals (`elapsed`, `calls`), in display order.
    pub totals: Vec<SectionTotal>,
    /// Every column of the section. `DiffRow::cells` aligns to it.
    pub columns: Vec<DiffColumn>,
    pub base_coverage: Coverage,
    pub head_coverage: Coverage,
    /// The family that assessed this section. The body only carries sections
    /// whose family the policy lists; a section of a family the policy leaves
    /// out is not sent at all.
    pub family: FamilyJudgement,
    /// Every entity `ReportDiff::rows` lets through, sorted by the floor
    /// column's head value descending (removed rows last), else by volume,
    /// else head's order.
    pub rows: Vec<DiffRow>,
    /// Head entities missing from a truncated baseline: not comparable,
    /// never reported as added. Sorted.
    pub omitted_from_base: Vec<String>,
    /// Baseline entities missing from a truncated head. Sorted.
    pub omitted_from_head: Vec<String>,
    /// The comparison page with this section's tab open (`?tab=`).
    pub dashboard_url: String,
}

/// The resource a `DiffSection` covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffResource {
    Functions,
    Sql,
    Http,
    Server,
    Mutexes,
    RwLocks,
    Channels,
    Io,
    /// A resource this client does not know.
    #[serde(other)]
    Unknown,
}

/// Which table of a resource a `DiffSection` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Alloc,
    Timing,
    /// The one table of a resource that has only one.
    Main,
    /// A kind this client does not know.
    #[serde(other)]
    Unknown,
}

/// One total of a `DiffSection`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionTotal {
    /// What the total is (`elapsed`, `calls`).
    pub label: String,
    pub unit: Unit,
    pub value: Change,
}

/// One column of a `DiffSection`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffColumn {
    /// The policy's name for the column (`total`, `p95`, `wait_p99`): what
    /// `metrics = [...]` lists.
    pub key: String,
    /// The header a reader sees.
    pub label: String,
    pub unit: Unit,
    /// Which way is worse; `None` for a neutral column.
    pub worse: Option<Direction>,
    /// A context column the assessment reads by role, never judges.
    pub role: Option<ColumnRole>,
}

/// What a value in a column is. The unit is the value's scale on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// A plain count of calls.
    Calls,
    /// Nanoseconds.
    Duration,
    Bytes,
    /// Allocation count.
    Count,
    /// Percent points (`12.34`), not basis points.
    Percent,
    /// A ratio, as the report carries it.
    Rate,
    /// Bytes per second.
    Throughput,
    /// A unit this client does not know.
    #[serde(other)]
    Unknown,
}

/// A direction a value moves in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    /// A direction this client does not know.
    #[serde(other)]
    Unknown,
}

/// The role of a context column in the assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnRole {
    /// `% Total`: the policy's `min_percent_total` reads it.
    Floor,
    /// `calls`: the policy's `min_calls` reads it.
    Volume,
    /// A role this client does not know.
    #[serde(other)]
    Unknown,
}

/// How much of a section one report lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    /// Entries the report lists.
    pub included: u64,
    /// Entries measured, including ones cut by `HOTPATH_META_UPLOAD_LIMIT`.
    pub total: u64,
}

/// A policy family's rules and tallies for one `DiffSection`: one per
/// section, and every section a body carries has one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyJudgement {
    pub name: FamilyName,
    /// Whether this family decides the verdict and reaches the PR comment.
    /// `false`: assessed and shown (outcomes, `crossed`), never counted.
    pub judged: bool,
    /// The family's bar, in percent.
    pub min_percent_change: f64,
    /// The policy's `metrics`, as indices into `DiffSection::columns`, in
    /// policy order.
    pub metric_columns: Vec<u32>,
    pub counts: OutcomeCounts,
}

/// A policy family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyName {
    Alloc,
    Timing,
    Flow,
    /// A family this client does not know.
    #[serde(other)]
    Unknown,
}

/// How many rows of a section ended in each `RowOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeCounts {
    pub ignored: u64,
    pub below_floor: u64,
    pub added: u64,
    pub removed: u64,
    pub too_few_calls: u64,
    pub regressions: u64,
    pub improvements: u64,
    pub unchanged: u64,
}

/// One entity of a `DiffSection`, both sides of every column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffRow {
    /// What the sides were matched on and `ignore` patterns run against.
    pub key: String,
    /// What a reader sees (the key itself for functions).
    pub name: String,
    /// Head's location when head has the entity, else the baseline's.
    pub location: Option<JsonLocation>,
    pub presence: Presence,
    /// The family's conclusion. Every row of every section sent has one,
    /// unjudged families included.
    pub outcome: RowOutcome,
    /// Aligned to `DiffSection::columns`. `None` when a present side's value
    /// did not parse.
    pub cells: Vec<Option<DiffCell>>,
}

/// Which reports carry a `DiffRow`'s entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Both,
    /// Only in head.
    Added,
    /// Only in the baseline.
    Removed,
    /// A presence this client does not know.
    #[serde(other)]
    Unknown,
}

/// A family's conclusion about one `DiffRow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowOutcome {
    /// Matched an `ignore` pattern.
    Ignored,
    /// Under the policy's `min_percent_total`.
    BelowFloor,
    Added,
    Removed,
    /// Under the policy's `min_calls`.
    TooFewCalls,
    Regression,
    Improvement,
    Unchanged,
    /// An outcome this client does not know.
    #[serde(other)]
    Unknown,
}

/// Both sides of one column of one `DiffRow`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DiffCell {
    /// `None` for an `Added` row: the baseline has no such entity. Never a
    /// stand-in zero.
    pub base: Option<f64>,
    /// `None` for a `Removed` row.
    pub head: Option<f64>,
    /// `None` when a side is absent.
    pub change_percent: Option<f64>,
    /// Set on a cell of one of the family's metrics that crossed the family's
    /// bar, in every family sent: which way the value moved. It counts toward
    /// the verdict only when the family is `judged` and the direction is the
    /// worse one.
    pub crossed: Option<Direction>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::json::cloud_api::{
        normalize_base_url, validate_benchmark_name, ApiError, ApiErrorCode, AuthStatus,
        BenchmarkList, BenchmarkSummary, BoundKind, BudgetCheck, BudgetEntity, BudgetFinding,
        ColumnRole, CommentOutcome, DiffCell, DiffResource, DiffResult, DiffSide, Direction,
        FamilyName, PolicyLevel, PolicyProblem, PolicyRejected, PolicySaved, PolicyUpdate,
        PolicyView, Presence, RepoList, Report, ReportDiff, ReportSummary, Repository, RowFilter,
        RowOutcome, SectionKind, TokenStatus, Unit, UploadCreated, Verdict, DEFAULT_BASE_URL,
    };
    use crate::json::JsonLocation;
    use crate::output::ProfilingMode;
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

    /// The `compared` part of a `ReportDiff` fixture: one regressed
    /// `functions` alloc section with a crossed `Both` row, an `Added` row, a
    /// `Removed` row and an unparseable cell, plus an unjudged `sql` section
    /// whose crossed regression the verdict does not count.
    const COMPARED_RESULT: &str = r#"{
        "status": "compared",
        "totals": {
            "elapsed": {"base": 1200000000.0, "head": 1250000000.0, "change_percent": 4.1666},
            "allocated": null,
            "peak_rss": {"base": 0.0, "head": 4096.0, "change_percent": 100.0}
        },
        "sections": [
            {
                "resource": "functions",
                "kind": "alloc",
                "mode": "alloc-bytes",
                "totals": [{"label": "elapsed", "unit": "duration", "value": {"base": 10.0, "head": 12.0, "change_percent": 20.0}}],
                "columns": [
                    {"key": "calls", "label": "Calls", "unit": "calls", "worse": null, "role": "volume"},
                    {"key": "total", "label": "Total", "unit": "bytes", "worse": "up", "role": null},
                    {"key": "percent_total", "label": "% Total", "unit": "percent", "worse": null, "role": "floor"}
                ],
                "base_coverage": {"included": 3, "total": 3},
                "head_coverage": {"included": 3, "total": 5},
                "family": {
                    "name": "alloc",
                    "judged": true,
                    "min_percent_change": 5.0,
                    "metric_columns": [1],
                    "counts": {"ignored": 0, "below_floor": 0, "added": 1, "removed": 1, "too_few_calls": 0, "regressions": 1, "improvements": 0, "unchanged": 0}
                },
                "rows": [
                    {
                        "key": "app::parse",
                        "name": "app::parse",
                        "location": {"file": "src/parse.rs", "line": 12, "column": 1},
                        "presence": "both",
                        "outcome": "regression",
                        "cells": [
                            {"base": 100.0, "head": 100.0, "change_percent": 0.0, "crossed": null},
                            {"base": 2048.0, "head": 3136.0, "change_percent": 53.125, "crossed": "up"},
                            null
                        ]
                    },
                    {
                        "key": "app::new_fn",
                        "name": "app::new_fn",
                        "location": null,
                        "presence": "added",
                        "outcome": "added",
                        "cells": [
                            {"base": null, "head": 3.0, "change_percent": null, "crossed": null},
                            {"base": null, "head": 512.0, "change_percent": null, "crossed": null},
                            {"base": null, "head": 1.5, "change_percent": null, "crossed": null}
                        ]
                    },
                    {
                        "key": "app::old_fn",
                        "name": "app::old_fn",
                        "location": {"file": "src/old.rs", "line": 3, "column": 5},
                        "presence": "removed",
                        "outcome": "removed",
                        "cells": [
                            {"base": 7.0, "head": null, "change_percent": null, "crossed": null},
                            {"base": 64.0, "head": null, "change_percent": null, "crossed": null},
                            {"base": 0.25, "head": null, "change_percent": null, "crossed": null}
                        ]
                    }
                ],
                "omitted_from_base": [],
                "omitted_from_head": ["app::cut"],
                "dashboard_url": "https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a/diff?tab=alloc"
            },
            {
                "resource": "sql",
                "kind": "main",
                "mode": null,
                "totals": [],
                "columns": [{"key": "p95", "label": "P95", "unit": "duration", "worse": "up", "role": null}],
                "base_coverage": {"included": 1, "total": 1},
                "head_coverage": {"included": 1, "total": 1},
                "family": {
                    "name": "timing",
                    "judged": false,
                    "min_percent_change": 20.0,
                    "metric_columns": [0],
                    "counts": {"ignored": 0, "below_floor": 0, "added": 0, "removed": 0, "too_few_calls": 0, "regressions": 1, "improvements": 0, "unchanged": 0}
                },
                "rows": [
                    {
                        "key": "SELECT 1",
                        "name": "SELECT 1",
                        "location": null,
                        "presence": "both",
                        "outcome": "regression",
                        "cells": [{"base": 1000.0, "head": 1500.0, "change_percent": 50.0, "crossed": "up"}]
                    }
                ],
                "omitted_from_base": [],
                "omitted_from_head": [],
                "dashboard_url": "https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a/diff?tab=sql"
            }
        ],
        "skipped": ["functions timing: the reports measured different percentiles."],
        "notes": ["io is disabled by the policy."]
    }"#;

    const DIFF_URL: &str = "https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/ci/reports/0199a3c2-7d2e-7b41-9c3a-1f2e3d4c5b6a/diff";

    /// The verdict of `COMPARED_RESULT` under a policy without budgets.
    const COMPARED_VERDICT: &str = r#"{"judged": true, "regressed": true, "regressions": 1, "improvements": 0, "budgets_broken": 0}"#;
    /// Nothing judged: no comparison and no budget rules, or no head.
    const UNJUDGED_VERDICT: &str = r#"{"judged": false, "regressed": false, "regressions": 0, "improvements": 0, "budgets_broken": 0}"#;
    /// What a policy without budgets yields for a head that parses.
    const NO_BUDGETS: &str = r#"{"rules": 0, "broken": 0, "findings": [], "notes": []}"#;

    /// Two rules as `rows=all` lists them: a broken `alloc` bound with a
    /// `message`, and a `calls` rule that holds.
    const BUDGETS: &str = r#"{
        "rules": 2,
        "broken": 1,
        "findings": [
            {
                "resource": "functions",
                "rule": 0,
                "pattern": "app::parse*",
                "message": "parse must stay under 1 KB per call",
                "entity": {"key": "app::parse", "name": "app::parse", "location": {"file": "src/parse.rs", "line": 12, "column": 1}},
                "check": {"on": "column", "family": "alloc", "kind": "alloc", "column": "avg"},
                "bound": "max",
                "unit": "bytes",
                "limit": 1024.0,
                "actual": 3136.0,
                "broken": true
            },
            {
                "resource": "sql",
                "rule": 0,
                "pattern": "SELECT 1",
                "message": null,
                "entity": {"key": "SELECT 1", "name": "SELECT 1", "location": null},
                "check": {"on": "count", "name": "count", "implied": false},
                "bound": "min",
                "unit": "calls",
                "limit": 10.0,
                "actual": 12.0,
                "broken": false
            }
        ],
        "notes": ["functions budget 1: the report lacks p99.9."]
    }"#;

    /// One rule whose entity never ran: the implied minimum is the broken
    /// check.
    const IMPLIED_MINIMUM_BUDGETS: &str = r#"{
        "rules": 1,
        "broken": 1,
        "findings": [
            {
                "resource": "functions",
                "rule": 0,
                "pattern": "app::gone",
                "message": null,
                "entity": null,
                "check": {"on": "count", "name": "calls", "implied": true},
                "bound": "min",
                "unit": "calls",
                "limit": 1.0,
                "actual": 0.0,
                "broken": true
            }
        ],
        "notes": []
    }"#;

    fn report_diff(base: &str, verdict: &str, budgets: &str, result: &str) -> serde_json::Value {
        let body = format!(
            r#"{{"repository":"pawurb/hotpath-rs","benchmark":"ci","head":{PR_SUMMARY},"base":{base},"policy":{{"level":"benchmark","fallback":null}},"rows":"all","verdict":{verdict},"budgets":{budgets},"result":{result},"dashboard_url":"{DIFF_URL}"}}"#
        );
        serde_json::from_str(&body).unwrap()
    }

    /// `COMPARED_RESULT` under a policy without budgets.
    fn compared_diff(result: &str) -> serde_json::Value {
        report_diff(&recorded_base(), COMPARED_VERDICT, NO_BUDGETS, result)
    }

    fn recorded_base() -> String {
        format!(r#"{{"report":{PUSH_SUMMARY},"branch_point":true}}"#)
    }

    /// Parses `value` as a `ReportDiff` and checks it re-serializes equal.
    fn round_trip(value: &serde_json::Value) -> ReportDiff {
        let diff: ReportDiff = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(&serde_json::to_value(&diff).unwrap(), value);
        diff
    }

    #[test]
    fn report_diff_compared_round_trips() {
        let value = compared_diff(COMPARED_RESULT);
        let diff = round_trip(&value);
        assert_eq!(diff.head, pr_summary());
        let base = diff.base.expect("compared has a base");
        assert!(base.branch_point);
        assert_eq!(diff.dashboard_url, DIFF_URL);
        assert_eq!(diff.policy.level, PolicyLevel::Benchmark);
        assert_eq!(diff.rows, RowFilter::All);

        assert_eq!(diff.budgets.expect("head parses").rules, 0);
        let DiffResult::Compared(comparison) = diff.result else {
            panic!("not compared: {:?}", diff.result);
        };
        assert_eq!(comparison.totals.allocated, None);
        let alloc = &comparison.sections[0];
        assert_eq!(alloc.mode, Some(ProfilingMode::AllocBytes));
        assert_eq!(alloc.columns[2].role, Some(ColumnRole::Floor));
        assert_eq!(alloc.family.name, FamilyName::Alloc);
        assert!(alloc.family.judged);

        let [both, added, removed] = &alloc.rows[..] else {
            panic!("three rows expected");
        };
        assert_eq!(both.outcome, RowOutcome::Regression);
        assert_eq!(
            both.cells[1],
            Some(DiffCell {
                base: Some(2048.0),
                head: Some(3136.0),
                change_percent: Some(53.125),
                crossed: Some(Direction::Up),
            })
        );
        assert_eq!(both.cells[2], None, "an unparseable cell is null");
        assert_eq!(added.presence, Presence::Added);
        assert!(added.cells.iter().all(|c| c.unwrap().base.is_none()));
        assert_eq!(removed.presence, Presence::Removed);
        assert!(removed.cells.iter().all(|c| c.unwrap().head.is_none()));

        let sql = &comparison.sections[1];
        assert!(!sql.family.judged);
        assert_eq!(sql.rows[0].outcome, RowOutcome::Regression);
        assert_eq!(sql.rows[0].cells[0].unwrap().crossed, Some(Direction::Up));
        assert_eq!(diff.verdict.regressions, 1, "unjudged rows never count");
    }

    #[test]
    fn report_diff_compared_with_budgets_round_trips() {
        let verdict = r#"{"judged": true, "regressed": true, "regressions": 1, "improvements": 0, "budgets_broken": 1}"#;
        let diff = round_trip(&report_diff(
            &recorded_base(),
            verdict,
            BUDGETS,
            COMPARED_RESULT,
        ));
        assert_eq!(
            diff.verdict,
            Verdict {
                judged: true,
                regressed: true,
                regressions: 1,
                improvements: 0,
                budgets_broken: 1,
            }
        );

        let budgets = diff.budgets.expect("head parses");
        assert_eq!(budgets.rules, 2);
        assert_eq!(budgets.broken, 1);
        assert_eq!(budgets.notes.len(), 1);
        let [broken, holds] = &budgets.findings[..] else {
            panic!("two findings expected");
        };
        assert_eq!(
            broken,
            &BudgetFinding {
                resource: DiffResource::Functions,
                rule: 0,
                pattern: "app::parse*".into(),
                message: Some("parse must stay under 1 KB per call".into()),
                entity: Some(BudgetEntity {
                    key: "app::parse".into(),
                    name: "app::parse".into(),
                    location: Some(JsonLocation {
                        file: "src/parse.rs".into(),
                        line: 12,
                        column: 1,
                    }),
                }),
                check: BudgetCheck::Column {
                    family: FamilyName::Alloc,
                    kind: SectionKind::Alloc,
                    column: "avg".into(),
                },
                bound: BoundKind::Max,
                unit: Unit::Bytes,
                limit: 1024.0,
                actual: 3136.0,
                broken: true,
            }
        );
        assert!(!holds.broken);
        assert_eq!(holds.bound, BoundKind::Min);
        assert_eq!(
            holds.check,
            BudgetCheck::Count {
                name: "count".into(),
                implied: false,
            }
        );
    }

    #[test]
    fn report_diff_broken_implied_minimum_round_trips() {
        let verdict = r#"{"judged": true, "regressed": true, "regressions": 0, "improvements": 0, "budgets_broken": 1}"#;
        let value = report_diff(
            "null",
            verdict,
            IMPLIED_MINIMUM_BUDGETS,
            r#"{"status":"no_baseline"}"#,
        );
        assert_eq!(value["budgets"]["findings"][0]["check"]["on"], "count");
        let diff = round_trip(&value);
        assert!(diff.verdict.regressed);

        let finding = &diff.budgets.expect("head parses").findings[0];
        assert_eq!(finding.entity, None);
        assert_eq!(
            finding.check,
            BudgetCheck::Count {
                name: "calls".into(),
                implied: true,
            }
        );
        assert_eq!(finding.actual, 0.0);
        assert!(finding.broken);
    }

    #[test]
    fn report_diff_requires_the_top_level_verdict() {
        // The shape before budgets: the verdict inside `compared`.
        let mut value = compared_diff(COMPARED_RESULT);
        let verdict = value.as_object_mut().unwrap().remove("verdict").unwrap();
        value["result"]["verdict"] = verdict;
        let error = serde_json::from_value::<ReportDiff>(value)
            .unwrap_err()
            .to_string();
        assert!(error.contains("verdict"), "{error}");
    }

    #[test]
    fn report_diff_rejects_an_unknown_budget_check() {
        let mut value = report_diff(
            "null",
            COMPARED_VERDICT,
            IMPLIED_MINIMUM_BUDGETS,
            r#"{"status":"no_baseline"}"#,
        );
        value["budgets"]["findings"][0]["check"]["on"] = "ratio".into();
        assert!(serde_json::from_value::<ReportDiff>(value).is_err());
    }

    #[test]
    fn report_diff_requires_a_family_and_an_outcome() {
        for (path, broken) in [
            ("/result/sections/1/family", serde_json::Value::Null),
            ("/result/sections/1/rows/0/outcome", serde_json::Value::Null),
        ] {
            let mut value = compared_diff(COMPARED_RESULT);
            *value.pointer_mut(path).unwrap() = broken;
            assert!(
                serde_json::from_value::<ReportDiff>(value).is_err(),
                "{path} null"
            );
        }
        let mut value = compared_diff(COMPARED_RESULT);
        value["result"]["sections"][1]["rows"][0]
            .as_object_mut()
            .unwrap()
            .remove("outcome");
        assert!(serde_json::from_value::<ReportDiff>(value).is_err());
    }

    #[test]
    fn report_diff_no_baseline_round_trips() {
        // Budgets that hold are the answer when there is no baseline. Under
        // `findings` a rule that holds leaves no finding, only `rules`.
        let verdict = r#"{"judged": true, "regressed": false, "regressions": 0, "improvements": 0, "budgets_broken": 0}"#;
        let budgets = r#"{"rules": 2, "broken": 0, "findings": [], "notes": []}"#;
        let mut value = report_diff("null", verdict, budgets, r#"{"status":"no_baseline"}"#);
        value["rows"] = "findings".into();
        let diff = round_trip(&value);
        assert_eq!(diff.rows, RowFilter::Findings);
        assert_eq!(diff.base, None);
        assert_eq!(diff.result, DiffResult::NoBaseline);
        assert_eq!(diff.dashboard_url, DIFF_URL);
        assert!(diff.verdict.judged);
        assert!(!diff.verdict.regressed);
        assert_eq!(diff.budgets.expect("head parses").rules, 2);
    }

    #[test]
    fn report_diff_no_baseline_without_budgets_judges_nothing() {
        let diff = round_trip(&report_diff(
            "null",
            UNJUDGED_VERDICT,
            NO_BUDGETS,
            r#"{"status":"no_baseline"}"#,
        ));
        assert_eq!(diff.budgets.expect("head parses").rules, 0);
        assert!(!diff.verdict.judged);
    }

    #[test]
    fn report_diff_unreadable_head_has_no_budgets() {
        let value = report_diff(
            &recorded_base(),
            UNJUDGED_VERDICT,
            "null",
            r#"{"status":"unreadable","side":"head","hotpath_version":null,"error":"missing field `functions_timing`"}"#,
        );
        assert!(value["budgets"].is_null());
        let diff = round_trip(&value);
        assert_eq!(diff.budgets, None);
        assert!(!diff.verdict.judged);
        assert!(matches!(
            diff.result,
            DiffResult::Unreadable {
                side: DiffSide::Head,
                ..
            }
        ));
    }

    #[test]
    fn report_diff_unreadable_round_trips() {
        let base = format!(r#"{{"report":{PUSH_SUMMARY},"branch_point":false}}"#);
        let value = report_diff(
            &base,
            UNJUDGED_VERDICT,
            NO_BUDGETS,
            r#"{"status":"unreadable","side":"base","hotpath_version":"0.20.0","error":"missing field `functions_timing`"}"#,
        );
        let diff = round_trip(&value);
        assert!(!diff.base.unwrap().branch_point);
        assert_eq!(
            diff.result,
            DiffResult::Unreadable {
                side: DiffSide::Base,
                hotpath_version: Some("0.20.0".into()),
                error: "missing field `functions_timing`".into(),
            }
        );
    }

    #[test]
    fn report_diff_tolerates_unknown_enum_values_and_fields() {
        let result = COMPARED_RESULT
            .replacen(r#""resource": "functions""#, r#""resource": "gpu""#, 1)
            .replacen(r#""unit": "bytes""#, r#""unit": "watts""#, 1)
            .replacen(r#""kind": "alloc""#, r#""kind": "energy", "later": [1]"#, 1)
            .replacen(r#""presence": "both""#, r#""presence": "moved""#, 1)
            .replacen(r#""outcome": "regression""#, r#""outcome": "flaky""#, 1);
        let budgets = BUDGETS
            .replacen(r#""resource": "functions""#, r#""resource": "gpu""#, 1)
            .replacen(r#""bound": "max""#, r#""bound": "between", "later": 1"#, 1)
            .replacen(r#""family": "alloc""#, r#""family": "energy""#, 1);
        let mut value = report_diff(&recorded_base(), COMPARED_VERDICT, &budgets, &result);
        value["later"] = serde_json::json!({"anything": true});
        value["rows"] = "sampled".into();
        value["verdict"]["later"] = true.into();

        let diff: ReportDiff = serde_json::from_value(value).unwrap();
        assert_eq!(diff.rows, RowFilter::Unknown);
        let finding = &diff.budgets.as_ref().unwrap().findings[0];
        assert_eq!(finding.resource, DiffResource::Unknown);
        assert_eq!(finding.bound, BoundKind::Unknown);
        assert!(matches!(
            finding.check,
            BudgetCheck::Column {
                family: FamilyName::Unknown,
                ..
            }
        ));
        let DiffResult::Compared(comparison) = diff.result else {
            panic!("not compared");
        };
        let section = &comparison.sections[0];
        assert_eq!(section.resource, DiffResource::Unknown);
        assert_eq!(section.kind, SectionKind::Unknown);
        assert_eq!(section.columns[1].unit, Unit::Unknown);
        assert_eq!(section.rows[0].presence, Presence::Unknown);
        assert_eq!(section.rows[0].outcome, RowOutcome::Unknown);
    }
}
