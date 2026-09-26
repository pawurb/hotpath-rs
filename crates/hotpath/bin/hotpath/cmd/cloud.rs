//! `hotpath cloud ...`: the hotpath.rs API client, for coding agents, CI
//! jobs and people who want structured JSON instead of the dashboard. Every
//! command prints the server's body as JSON on stdout, every failure as one
//! JSON document on stderr, and exits with a code a script can branch on
//! without parsing (see `api`). One file per command
//! under `cloud/`, shared HTTP / auth plumbing in `cloud/api.rs`.

mod api;
mod auth;
mod benchmarks;
mod policy;
mod repo;
mod report;
mod repos;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cmd::cloud::api::{CliError, Output};
use crate::cmd::cloud::policy::{PolicyScope, SetPolicyArgs};
use crate::cmd::cloud::report::ReportArgs;

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

    #[command(
        about = "Fetch one stored report by pull request, commit or id",
        long_about = "Fetch one stored report by pull request, commit or id
(GET /api/v1/repos/{owner}/{name}/benchmarks/{benchmark}/reports/latest?pr=N|commit=SHA,
or .../reports/{id}).

--pr and --commit answer the newest matching report, newest by upload. --commit takes
a full 40-character sha and matches the measured commit or the pull request's head
commit: a pull request job usually measures GitHub's merge commit, which nobody has
locally, so `--commit $(git rev-parse HEAD)` on a PR branch still finds the PR's
report. --event push|pull_request narrows --pr / --commit to one event (the same
commit is often measured by a push to main and as a PR head). --no-payload asks for
the summary only; the payload is the uploaded hotpath JSON report, verbatim apart
from key order.

A report of another benchmark, an unknown id, no match yet and a repository the
token's user cannot see all answer 404 `not_found`: poll with --commit until it
exits 0 to wait for CI's upload. Same token and base URL environment as `auth`."
    )]
    Report(ReportArgs),

    #[command(
        about = "Show the PR comment policy in force for a repository or one benchmark",
        long_about = "Show the PR comment policy in force for a repository or one benchmark
(GET /api/v1/repos/{owner}/{name}/policy, or .../benchmarks/{benchmark}/policy).

A repo policy applies to every benchmark of the repository; a benchmark policy
overrides it for one benchmark. Levels do not inherit from each other: the one that
applies is the benchmark's if stored, else the repo's if stored, else the built-in
default, and any key a stored document omits takes the built-in value. `level` says
which document `source` is, `stored` whether the scope asked about has its own (when
false, `source` is what it inherits: a starting point for an edit), and `fallback` is
set when the stored document no longer parses and the built-in default judges
instead. Same token and base URL environment as `auth`."
    )]
    GetPolicy(PolicyScope),

    #[command(
        about = "Replace the PR comment policy of a repository or one benchmark",
        long_about = "Replace the PR comment policy of a repository or one benchmark
(PUT /api/v1/repos/{owner}/{name}/policy, or .../benchmarks/{benchmark}/policy).

--file is the whole TOML document (`-` reads stdin), stored as written: it replaces
what the scope has, nothing is merged. The document is not parsed here; the server
judges it. At most 65536 bytes, not blank. --dry-run validates without storing.

Writing needs push permission on the repository (403 `forbidden` otherwise), and a
benchmark must exist, created by its first upload, to hold a policy (404
`not_found`). A refused document exits 1 with the server's 422 body on stderr:
`code` is `invalid_policy` and `problems` lists every problem found, each with its
`line` when the server can name one. A TOML syntax error stops parsing, so it is
reported alone; otherwise every structural and range problem comes in one answer.
Same token and base URL environment as `auth`."
    )]
    SetPolicy(SetPolicyArgs),
}

impl CloudArgs {
    /// Runs the command; every failure is one JSON document on stderr and exit 1.
    /// Each command validates its arguments before it builds the client, so a
    /// bad argument is reported without a token and never costs a request.
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
        match cmd {
            CloudCommand::Auth => auth::run(output),
            CloudCommand::Repos => repos::run(output),
            CloudCommand::Benchmarks { repo } => benchmarks::run(output, &repo),
            CloudCommand::Report(args) => report::run(output, args),
            CloudCommand::GetPolicy(scope) => policy::get(output, scope),
            CloudCommand::SetPolicy(args) => policy::set(output, args),
        }
    }
}
