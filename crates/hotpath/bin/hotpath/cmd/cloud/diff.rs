//! `hotpath cloud diff`: one stored report against its baseline, judged by
//! the server under the benchmark's current policy, the same judgement the PR
//! comment shows but as structured JSON. The head is picked like `report`
//! picks a report (`selector.rs`); `--base ID` overrides the baseline the
//! server recorded for it at upload. Sections carry only regressed and
//! improved rows unless `--full` asks for every row (`RowFilter`).
//!
//! The body goes to stdout on exit 0, 3 and 4: a regression or a missing
//! baseline is an answer, not an error, and the caller needs the body to act
//! on it. The exit code is read from the deserialized body, never from the
//! HTTP status, which is 200 for every `DiffResult`.

use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::{DiffResult, ReportDiff};

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;
use crate::cmd::cloud::selector::{validate_id, ReportSelector, Selector};

/// Compared, and the policy calls it a regression.
const EXIT_REGRESSION: u8 = 3;
/// Nothing was judged: no baseline, or a side the server cannot read.
const EXIT_NOT_JUDGED: u8 = 4;

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
        value_name = "ID",
        help = "Compare against the report with this id instead of the recorded baseline"
    )]
    base: Option<String>,

    #[arg(
        long,
        help = "Every row of every section, not only regressions and improvements"
    )]
    full: bool,
}

pub(crate) fn run(output: &Output, args: DiffArgs) -> Result<ExitCode, CliError> {
    let path = request_path(&args)?;
    let client = Client::from_env()?;
    let diff: ReportDiff = client.get(&path)?;
    output.emit(&diff)?;
    Ok(exit_code(&diff.result))
}

fn exit_code(result: &DiffResult) -> ExitCode {
    match result {
        DiffResult::Compared(comparison) if comparison.verdict.regressed => {
            ExitCode::from(EXIT_REGRESSION)
        }
        DiffResult::Compared(_) => ExitCode::SUCCESS,
        DiffResult::NoBaseline | DiffResult::Unreadable { .. } => ExitCode::from(EXIT_NOT_JUDGED),
    }
}

/// The request path and query for validated `args`, or the error naming the
/// first bad argument.
fn request_path(args: &DiffArgs) -> Result<String, CliError> {
    let repo = repo::validate(&args.repo)?;
    let benchmark = repo::validate_benchmark(&args.benchmark)?;
    let selector = args.selector.validate()?;
    let base = args
        .base
        .as_deref()
        .map(|id| validate_id("--base", id))
        .transpose()?;

    let mut query = vec![match selector {
        Selector::Pr(pr) => format!("pr={pr}"),
        Selector::Commit(sha) => format!("commit={sha}"),
        Selector::Id(id) => format!("head={id}"),
    }];
    // clap rejects `--event` together with `--id`.
    if let Some(event) = args.selector.event {
        query.push(format!("event={}", event.as_str()));
    }
    if let Some(base) = base {
        query.push(format!("base={base}"));
    }
    // Without `rows=` the server sends findings only.
    if args.full {
        query.push("rows=all".to_string());
    }
    Ok(format!(
        "/api/v1/repos/{repo}/benchmarks/{benchmark}/diff?{}",
        query.join("&")
    ))
}
