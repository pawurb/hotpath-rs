//! `hotpath cloud repos`: prints the `GET /api/v1/repos` body, every
//! repository the token reaches (with its benchmarks), so an agent can pick
//! one before naming it in a later command.

use std::process::ExitCode;

use hotpath::json::cloud_api::RepoList;

use crate::cmd::cloud::api::{CliError, Client, Output};

pub(crate) fn run(client: &Client, output: &Output) -> Result<ExitCode, CliError> {
    let repos: RepoList = client.get("/api/v1/repos")?;
    output.emit(&repos)?;
    Ok(ExitCode::SUCCESS)
}
