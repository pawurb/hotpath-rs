//! `hotpath cloud ...`: the hotpath.rs read API client, for coding agents, CI
//! jobs and people who want structured JSON instead of the dashboard. Every
//! command prints the server's body as JSON on stdout and exits with a code a
//! script can branch on without parsing (see `api`). One file per command
//! under `cloud/`, shared HTTP / auth plumbing in `cloud/api.rs`.

mod api;
mod auth;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cmd::cloud::api::{Client, Output};

#[derive(Parser, Debug)]
pub struct CloudArgs {
    #[command(subcommand)]
    pub cmd: CloudCommand,

    #[arg(long, global = true, help = "Indent the JSON output")]
    pub pretty: bool,

    #[arg(
        long,
        global = true,
        value_name = "FILE",
        help = "Write the JSON output to FILE instead of stdout"
    )]
    pub output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum CloudCommand {
    #[command(
        about = "Show which user and token this shell acts as (GET /api/v1/auth)",
        long_about = "Show which user and token this shell acts as (GET /api/v1/auth).

Reads the token from HOTPATH_API_TOKEN (create one at https://hotpath.rs/app/tokens)
and the base URL from HOTPATH_API_URL (default https://hotpath.rs). Exits 1 with the
server's error sentence when the token does not work."
    )]
    Auth,
}

impl CloudArgs {
    /// Runs the command; every failure is one sentence on stderr and exit 1.
    pub fn run(self) -> ExitCode {
        match self.execute() {
            Ok(code) => code,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::FAILURE
            }
        }
    }

    fn execute(self) -> Result<ExitCode, String> {
        let client = Client::from_env()?;
        let output = Output {
            pretty: self.pretty,
            file: self.output,
        };
        match self.cmd {
            CloudCommand::Auth => auth::run(&client, &output),
        }
    }
}
