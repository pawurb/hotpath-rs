//! `hotpath cloud report`: one stored report, selected by pull request, commit
//! or id (`selector.rs`), with or without its payload. Also the agent loop's
//! polling primitive: "has CI uploaded a report for my commit yet" is exit 0
//! here instead of exit 1 with the server's `not_found`.

use std::process::ExitCode;

use clap::Args;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;
use crate::cmd::cloud::selector::{ReportSelector, Selector};

#[derive(Args, Debug)]
pub(crate) struct ReportArgs {
    #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
    repo: String,

    #[arg(long, value_name = "NAME", help = "The benchmark (series)")]
    benchmark: String,

    #[command(flatten)]
    selector: ReportSelector,

    #[arg(long, help = "Leave out the report payload (summary only)")]
    no_payload: bool,
}

pub(crate) fn run(output: &Output, args: ReportArgs) -> Result<ExitCode, CliError> {
    let path = request_path(&args)?;
    // The request (`payload=false`), not the client, decides the body's shape.
    let report = Client::from_env()?.get_raw(&path)?;
    output.emit_raw(&report)?;
    Ok(ExitCode::SUCCESS)
}

/// The request path and query for validated `args`, or the error naming the
/// first bad argument.
fn request_path(args: &ReportArgs) -> Result<String, CliError> {
    let repo = repo::validate(&args.repo)?;
    let benchmark = repo::validate_benchmark(&args.benchmark)?;
    let selector = args.selector.validate()?;

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
    if let Some(event) = args.selector.event {
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
