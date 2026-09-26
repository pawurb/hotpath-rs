//! `hotpath cloud auth`: the status probe, like `gh auth status`. Prints the
//! `GET /api/v1/auth` body (the login the token acts as, its name, its
//! expiry) and exits 1 with the server's error JSON when the token does not
//! work. Nothing else calls `/auth`.

use std::process::ExitCode;

use hotpath::json::cloud_api::AuthStatus;

use crate::cmd::cloud::api::{CliError, Client, Output};

pub(crate) fn run(output: &Output) -> Result<ExitCode, CliError> {
    let status: AuthStatus = Client::from_env()?.get("/api/v1/auth")?;
    output.emit(&status)?;
    Ok(ExitCode::SUCCESS)
}
