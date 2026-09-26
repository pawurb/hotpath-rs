//! `hotpath cloud report`: one stored report of a repository's benchmark,
//! selected by pull request, commit or id, with or without its payload. Also
//! the polling primitive of the agent loop: "has CI uploaded a report for my
//! commit yet" is exit 0 here instead of exit 1 with the server's `not_found`.
//!
//! Every argument is validated before the client is built, so a bad value is
//! reported without a token and never costs a request. The validated values
//! are URL-safe as they are and go into the path and query unencoded.

use std::num::NonZeroU64;
use std::process::ExitCode;

use clap::{ArgGroup, Args, ValueEnum};
use hotpath::json::cloud_api::{Report, ReportSummary};

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

#[derive(Args, Debug)]
#[command(group(
    ArgGroup::new("selector")
        .args(["pr", "commit", "id"])
        .required(true)
        .multiple(false)
))]
pub(crate) struct ReportArgs {
    #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
    repo: String,

    #[arg(long, value_name = "NAME", help = "The benchmark (series)")]
    benchmark: String,

    #[arg(long, value_name = "N", help = "Newest report of this pull request")]
    pr: Option<u64>,

    #[arg(
        long,
        value_name = "SHA",
        help = "Newest report measuring this full 40-character commit sha, or with it as PR head"
    )]
    commit: Option<String>,

    #[arg(long, value_name = "ID", help = "The report with this id (a uuid)")]
    id: Option<String>,

    #[arg(
        long,
        value_enum,
        conflicts_with = "id",
        help = "Only reports of this event (with --pr or --commit)"
    )]
    event: Option<Event>,

    #[arg(long, help = "Leave out the report payload (summary only)")]
    no_payload: bool,
}

/// The CI event a report was uploaded from, as `--event` narrows it.
#[derive(ValueEnum, Clone, Copy, Debug)]
enum Event {
    #[value(name = "push")]
    Push,
    #[value(name = "pull_request")]
    PullRequest,
}

impl Event {
    fn as_str(self) -> &'static str {
        match self {
            Event::Push => "push",
            Event::PullRequest => "pull_request",
        }
    }
}

/// Which report, after validation: a value here is safe in a URL as is.
enum Selector {
    /// Newest report of this pull request.
    Pr(NonZeroU64),
    /// Newest report whose measured commit or PR head is this lowercase sha.
    Commit(String),
    /// This report.
    Id(String),
}

pub(crate) fn run(output: &Output, args: ReportArgs) -> Result<ExitCode, CliError> {
    let path = request_path(&args)?;
    let client = Client::from_env()?;
    // The request, not the body, decides which type the answer has.
    if args.no_payload {
        let summary: ReportSummary = client.get(&path)?;
        output.emit(&summary)?;
    } else {
        let report: Report = client.get(&path)?;
        output.emit(&report)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The request path and query for validated `args`, or the error naming the
/// first bad argument.
fn request_path(args: &ReportArgs) -> Result<String, CliError> {
    let repo = repo::validate(&args.repo)?;
    let benchmark = repo::validate_benchmark(&args.benchmark)?;
    let selector = selector(args)?;

    let base = format!("/api/v1/repos/{repo}/benchmarks/{benchmark}/reports");
    let mut query = Vec::new();
    let path = match &selector {
        Selector::Pr(pr) => {
            query.push(format!("pr={pr}"));
            format!("{base}/latest")
        }
        Selector::Commit(sha) => {
            query.push(format!("commit={sha}"));
            format!("{base}/latest")
        }
        Selector::Id(id) => format!("{base}/{id}"),
    };
    // clap rejects `--event` together with `--id`.
    if let Some(event) = args.event {
        query.push(format!("event={}", event.as_str()));
    }
    if args.no_payload {
        query.push("payload=false".to_string());
    }
    Ok(if query.is_empty() {
        path
    } else {
        format!("{path}?{}", query.join("&"))
    })
}

fn selector(args: &ReportArgs) -> Result<Selector, CliError> {
    // The required, single-choice `selector` group guarantees exactly one.
    if let Some(pr) = args.pr {
        return NonZeroU64::new(pr).map(Selector::Pr).ok_or_else(|| {
            CliError::client("invalid --pr `0`: expected a pull request number greater than 0.")
        });
    }
    if let Some(commit) = &args.commit {
        let sha = commit.to_ascii_lowercase();
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CliError::client(format!(
                "invalid --commit `{commit}`: expected a full 40-character hex commit sha, e.g. --commit $(git rev-parse HEAD)."
            )));
        }
        return Ok(Selector::Commit(sha));
    }
    let id = args.id.as_deref().unwrap_or_default();
    if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(CliError::client(format!(
            "invalid --id `{id}`: expected a report id (a 36-character uuid)."
        )));
    }
    Ok(Selector::Id(id.to_string()))
}
