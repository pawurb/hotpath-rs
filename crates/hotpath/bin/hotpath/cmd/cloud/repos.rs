//! `hotpath cloud repos`: prints the `GET /api/v1/repos` body, every
//! repository the token reaches (with its benchmarks), so an agent can pick
//! one before naming it in a later command.

use std::process::ExitCode;

use crate::cmd::cloud::api::{CliError, Client, Output};

pub(crate) fn run(output: &Output) -> Result<ExitCode, CliError> {
    let repos = Client::from_env()?.get_raw("/api/v1/repos")?;
    output.emit_raw(&repos)?;
    Ok(ExitCode::SUCCESS)
}
