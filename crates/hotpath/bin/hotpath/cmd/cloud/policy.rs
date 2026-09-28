//! `hotpath cloud get-policy`: the policy the newest report of a repository
//! (`--repo`) or of one of its benchmarks (`--benchmark`) carried. A policy
//! belongs to a report, so there is nothing to write here. Every argument is
//! validated before the client is built, so a bad value is reported without
//! a token and never costs a request.

use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::PolicyView;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

/// The scope a command asks about: the repository, or one benchmark when
/// `--benchmark` is given.
#[derive(Args, Debug)]
pub(crate) struct PolicyScope {
    #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
    repo: String,

    #[arg(
        long,
        value_name = "NAME",
        help = "One benchmark's policy instead of the repository's"
    )]
    benchmark: Option<String>,
}

pub(crate) fn get(output: &Output, scope: PolicyScope) -> Result<ExitCode, CliError> {
    let path = request_path(&scope)?;
    let view: PolicyView = Client::from_env()?.get(&path)?;
    output.emit(&view)?;
    Ok(ExitCode::SUCCESS)
}

/// `.../policy` of the repository, or of the benchmark when one is named.
fn request_path(scope: &PolicyScope) -> Result<String, CliError> {
    let repo = repo::validate(&scope.repo)?;
    Ok(match &scope.benchmark {
        None => format!("/api/v1/repos/{repo}/policy"),
        Some(benchmark) => {
            let benchmark = repo::validate_benchmark(benchmark)?;
            format!("/api/v1/repos/{repo}/benchmarks/{benchmark}/policy")
        }
    })
}
