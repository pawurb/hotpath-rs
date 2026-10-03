//! `hotpath cloud diff`: one stored report against its upload-time baseline,
//! judged under the benchmark's current policy, as JSON. The head is picked
//! like `report` (`selector.rs`). Without flags only the PR comment's findings
//! are sent; `--full` (`rows=all`) asks for every row and cell, `--advisory`
//! also lists the unjudged families' findings.
//!
//! Exit 0 means exactly "judged and nothing failed"
//! (`verdict.judged && !verdict.regressed`), exit 1 is everything else. A
//! missing or unreadable baseline does not fail a report whose budgets hold.
//! The exit code is read from the body's top-level `verdict` (`DiffVerdict`),
//! never from the HTTP status, which is 200 for every result.

use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::Verdict;
use serde::Deserialize;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;
use crate::cmd::cloud::selector::{ReportSelector, Selector};

#[derive(Args, Debug)]
pub(crate) struct DiffArgs {
    #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
    repo: String,

    #[arg(long, value_name = "NAME", help = "The benchmark (series)")]
    benchmark: String,

    #[command(flatten)]
    selector: ReportSelector,

    #[arg(
        long,
        help = "Every row and every cell of every section, with the column legend, not only the PR comment's findings"
    )]
    full: bool,

    #[arg(
        long,
        conflicts_with = "full",
        help = "The findings of unjudged families too, which never count toward the verdict"
    )]
    advisory: bool,
}

pub(crate) fn run(output: &Output, args: DiffArgs) -> Result<ExitCode, CliError> {
    let path = request_path(&args)?;
    let client = Client::from_env()?;
    let diff = client.get_raw(&path)?;
    // Parsed before anything is printed: a body without a verdict prints
    // nothing on stdout.
    let DiffVerdict { verdict } = diff.parse()?;
    output.emit_raw(&diff)?;
    Ok(exit_code(&verdict))
}

/// The part of a diff body the exit code reads; serde ignores the rest.
#[derive(Deserialize)]
struct DiffVerdict {
    verdict: Verdict,
}

fn exit_code(verdict: &Verdict) -> ExitCode {
    if verdict.judged && !verdict.regressed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The request path and query for validated `args`, or the error naming the
/// first bad argument.
fn request_path(args: &DiffArgs) -> Result<String, CliError> {
    let repo = repo::validate(&args.repo)?;
    let benchmark = repo::validate_benchmark(&args.benchmark)?;
    let selector = args.selector.validate()?;

    let mut query = vec![match selector {
        Selector::Pr(pr) => format!("pr={pr}"),
        Selector::Commit(sha) => format!("commit={sha}"),
        Selector::Id(id) => format!("head={id}"),
    }];
    // clap rejects `--event` together with `--id`.
    if let Some(event) = args.selector.event {
        query.push(format!("event={}", event.as_str()));
    }
    // Without `rows=` the server sends findings only. clap rejects
    // `--advisory` together with `--full`.
    if args.full {
        query.push("rows=all".to_string());
    } else if args.advisory {
        query.push("rows=advisory".to_string());
    }
    Ok(format!(
        "/api/v1/repos/{repo}/benchmarks/{benchmark}/diff?{}",
        query.join("&")
    ))
}
