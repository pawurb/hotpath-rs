//! `hotpath cloud ...`: the hotpath.rs read API client, for coding agents, CI
//! jobs and people who want structured JSON instead of the dashboard. Every
//! command prints the server's body as JSON on stdout, every failure as one
//! JSON document on stderr, and exits with a code a script can branch on
//! without parsing (see `api`). One file per command
//! under `cloud/`, shared HTTP / auth plumbing in `cloud/api.rs`.

mod api;
mod auth;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cmd::cloud::api::{CliError, Client, Output};

#[derive(Parser, Debug)]
pub(crate) struct CloudArgs {
    #[command(subcommand)]
    pub(crate) cmd: CloudCommand,

    #[arg(long, global = true, help = "Indent the JSON output")]
    pub(crate) pretty: bool,

    #[arg(
        long,
        global = true,
        value_name = "FILE",
        help = "Write the JSON output to FILE instead of stdout"
    )]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CloudCommand {
    #[command(
        about = "Show which user and token this shell acts as (GET /api/v1/auth)",
        long_about = "Show which user and token this shell acts as (GET /api/v1/auth).

Reads the token from HOTPATH_API_TOKEN (create one at https://hotpath.rs/app/tokens)
and the base URL from HOTPATH_API_URL (default https://hotpath.rs). Exits 1 with the
server's error JSON on stderr when the token does not work."
    )]
    Auth,
}

impl CloudArgs {
    /// Runs the command; every failure is one JSON document on stderr and exit 1.
    pub(crate) fn run(self) -> ExitCode {
        let CloudArgs {
            cmd,
            pretty,
            output,
        } = self;
        let output = Output {
            pretty,
            file: output,
        };
        match Self::execute(cmd, &output) {
            Ok(code) => code,
            Err(error) => {
                output.emit_error(&error);
                ExitCode::FAILURE
            }
        }
    }

    fn execute(cmd: CloudCommand, output: &Output) -> Result<ExitCode, CliError> {
        let client = Client::from_env()?;
        match cmd {
            CloudCommand::Auth => auth::run(&client, output),
        }
    }
}
