//! `hotpath cloud benchmarks [--repo owner/name]`: prints the
//! `GET /api/v1/repos/{owner}/{name}/benchmarks` body for one repository,
//! resolved by `repo::resolve` (the flag, else the `origin` remote). The
//! body's `repository` is whatever the server calls it now and is printed as
//! is, even when it differs from the name requested.

use std::process::ExitCode;

use hotpath::json::cloud_api::BenchmarkList;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

pub(crate) fn run(
    client: &Client,
    output: &Output,
    repo_flag: Option<&str>,
) -> Result<ExitCode, CliError> {
    let repo = repo::resolve(repo_flag)?;
    let benchmarks: BenchmarkList = client.get(&format!("/api/v1/repos/{repo}/benchmarks"))?;
    output.emit(&benchmarks)?;
    Ok(ExitCode::SUCCESS)
}
