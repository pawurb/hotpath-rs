//! `hotpath cloud auth`: the status probe, like `gh auth status`. Prints the
//! `GET /api/v1/auth` body (the login the token acts as, its name, its
//! expiry) and exits 1 with the server's sentence when the token does not
//! work. Nothing else calls `/auth`.

use std::process::ExitCode;

use hotpath::json::cloud_api::AuthStatus;

use crate::cmd::cloud::api::{Client, Output};

pub(crate) fn run(client: &Client, output: &Output) -> Result<ExitCode, String> {
    let status: AuthStatus = client.get("/api/v1/auth")?;
    output.emit(&status)?;
    Ok(ExitCode::SUCCESS)
}
