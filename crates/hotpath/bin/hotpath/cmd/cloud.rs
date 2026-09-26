//! `hotpath cloud ...`: the hotpath.rs read API client, for coding agents, CI
//! jobs and people who want structured JSON instead of the dashboard. Every
//! command prints the server's body as JSON on stdout, every failure as one
//! JSON document on stderr, and exits with a code a script can branch on
//! without parsing (see `api`). One file per command
//! under `cloud/`, shared HTTP / auth plumbing in `cloud/api.rs`.

mod api;
mod auth;
mod benchmarks;
mod repo;
mod repos;

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

    #[command(
        about = "List the repositories the token reaches, with their benchmarks (GET /api/v1/repos)",
        long_about = "List the repositories the token reaches, with their benchmarks (GET /api/v1/repos).

Every active repository the token's user can see on GitHub with the hotpath App
installed, ordered by full name, each with its benchmarks (name, stored reports, time
of the newest report). Same token and base URL environment as `auth`."
    )]
    Repos,

    #[command(
        about = "List one repository's benchmarks (GET /api/v1/repos/{owner}/{name}/benchmarks)",
        long_about = "List one repository's benchmarks (GET /api/v1/repos/{owner}/{name}/benchmarks).

The repository is --repo owner/name, as `repos` lists it. A repository that does not
exist, that the token's user cannot see or that has no App installed all answer 404.
Same token and base URL environment as `auth`."
    )]
    Benchmarks {
        #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
        repo: String,
    },
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
            CloudCommand::Repos => repos::run(&client, output),
            CloudCommand::Benchmarks { repo } => benchmarks::run(&client, output, &repo),
        }
    }
}
