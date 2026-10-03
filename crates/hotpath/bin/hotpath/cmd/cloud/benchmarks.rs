//! `hotpath cloud benchmarks --repo owner/name`: prints the
//! `GET /api/v1/repos/{owner}/{name}/benchmarks` body. Its `repository` may
//! differ from the name requested.

use std::process::ExitCode;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

pub(crate) fn run(output: &Output, repo: &str) -> Result<ExitCode, CliError> {
    // Validated before the client is built: a bad value is reported even
    // without a token and never costs a request.
    let repo = repo::validate(repo)?;
    let benchmarks = Client::from_env()?.get_raw(&format!("/api/v1/repos/{repo}/benchmarks"))?;
    output.emit_raw(&benchmarks)?;
    Ok(ExitCode::SUCCESS)
}
