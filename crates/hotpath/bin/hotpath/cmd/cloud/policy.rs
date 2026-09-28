//! `hotpath cloud get-policy`: the policy one benchmark of a repository was
//! last judged under, the one carried by its newest `push` report, or by its
//! newest report of any event when it has no `push` report. A policy belongs
//! to a report, so there is nothing to write here. Every argument is
//! validated before the client is built, so a bad value is reported without
//! a token and never costs a request.

use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::PolicyView;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

#[derive(Args, Debug)]
pub(crate) struct PolicyArgs {
    #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
    repo: String,

    #[arg(long, value_name = "NAME", help = "The benchmark")]
    benchmark: String,
}

pub(crate) fn get(output: &Output, args: PolicyArgs) -> Result<ExitCode, CliError> {
    let repo = repo::validate(&args.repo)?;
    let benchmark = repo::validate_benchmark(&args.benchmark)?;
    let path = format!("/api/v1/repos/{repo}/benchmarks/{benchmark}/policy");
    let view: PolicyView = Client::from_env()?.get(&path)?;
    output.emit(&view)?;
    Ok(ExitCode::SUCCESS)
}
