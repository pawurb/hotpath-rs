//! `hotpath cloud ...`: the hotpath.rs API client, for coding agents, CI
//! jobs and people who want structured JSON instead of the dashboard. Every
//! command prints the server's body as JSON on stdout, every failure as one
//! JSON document on stderr, and exits with a code a script can branch on
//! without parsing (see `api`). One file per command
//! under `cloud/`, shared HTTP / auth plumbing in `cloud/api.rs`.

mod api;
mod auth;
mod benchmarks;
mod diff;
mod repo;
mod report;
mod repos;
mod selector;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cmd::cloud::api::{CliError, Output};
use crate::cmd::cloud::diff::DiffArgs;
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
from key order and without `meta.policy`.

`policy_path` names the policy file the report was judged under, relative to the
repository root, and `policy_url` is that file on GitHub, pinned to the measured
commit. Both are null only when the report carried no policy and the built-in default
judged; otherwise both are set. The document is the file at that path in the measured
commit; the API never returns it, with or without --no-payload.

A report of another benchmark, an unknown id, no match yet and a repository the
token's user cannot see all answer 404 `not_found`: poll with --commit until it
exits 0 to wait for CI's upload. Same token and base URL environment as `auth`."
    )]
    Report(ReportArgs),

    #[command(
        about = "Compare a stored report with its baseline under the policy it carried",
        long_about = "Compare a stored report with its baseline under the policy it carried
(GET /api/v1/repos/{owner}/{name}/benchmarks/{benchmark}/diff?pr=N|commit=SHA|head=ID).

The head report is selected exactly as `report` selects one: --pr and --commit
answer the newest matching report, newest by upload; --commit takes a full
40-character sha and matches the measured commit or the pull request's head
commit, so `--commit $(git rev-parse HEAD)` on a PR branch finds the PR's report;
--id names a report; --event push|pull_request narrows --pr / --commit.

The baseline is the one the server recorded for the head at upload, the one the PR
comment compared. The judging policy is the one the head report carried when it was
uploaded, the built-in default when it carried none (`policy.path` is then null), so
the answer for a stored report does not change with a later edit of the policy.
Every value is a string formatted as the PR comment shows it (\"2.06 ms\", \"3.1 KB\",
\"20\"), and its `unit` says how to read it. `change_percent` stays a number of
percent and is the exact figure to reason about: rounding can make `base` and `head`
read the same while it is not zero. A cell names its `column`, and `family.metrics`
names the columns the policy judges.

Each section is one metric family the policy lists; a family the policy leaves out is
not in the body at all. A judged family decides the verdict; an unjudged one is still
assessed (outcomes and crossed cells) but never counts. By default each section lists
only the rows the PR comment lists: regressions, improvements, added and removed rows
of judged families. Each of those rows carries only the cells that explain it: the
ones that crossed the family's bar (the family's metrics for an added or removed row)
and the context columns, which have a `role` (calls, % total). A column whose value
did not parse has no cell and is named in the row's `unreadable`. A section without
rows is still sent, with its `family` and `counts` but without its totals. The
verdict, the run totals and each family's `counts` are always complete, and `rows` in
the body says which filter applied. --full lists every row of every section, unjudged
families included, every cell of every row, the section totals and `columns`, the
legend that says which way is worse for each column. --advisory is the step between
the two: the default cut, plus the regressions, improvements, added and removed rows
of unjudged families, so a nonzero count in an unjudged family's `counts` can be
inspected without --full. Those rows never count toward the verdict; `family.judged`
of their section says so. Inside a section, a key that is unset or an empty list is
left out: `mode`, `totals`, `columns`, `omitted_from_base`, `omitted_from_head`, a
row's `location` and `unreadable`, a cell's `role` and `crossed`. A cell's `base`,
`head` and `change_percent` are always there, null when a side is absent, and so is
every key outside the sections (`base`, `budgets`, `policy.path`, `policy.fallback`,
the run totals).

Budgets are the policy's absolute bounds on named entities (`[[functions.budgets]]`,
`[[sql.budgets]]`, ...). They are judged on the head report alone, so a report without
a baseline (a push to main, a pull request whose base has no report yet) still gets an
answer. `budgets.rules` counts the policy's rules and `budgets.broken` the broken
checks; `budgets.findings` lists the broken ones, every check with --full, and
`budgets.notes` says what could not be checked. `budgets` is null when the head report
does not parse.

The top-level `verdict` covers the comparison and the budgets together: `judged` says
whether anything was judged, `regressed` whether a judged family regressed or a budget
is broken.

Exit codes:
  0  `verdict.judged` and not `verdict.regressed`
  1  everything else: something failed, nothing was judged, or any error (unknown
     report, bad argument value, auth, network, unparseable body)
  2  usage error
So 0 means exactly \"judged and fine\":
  0  compared, no regression, budgets hold (or the policy has none)
  1  compared, a regression (see `sections[].rows` whose `outcome` is `regression`
     and their `crossed` cells)
  1  compared, no regression, a budget broken
  0  no baseline or an unreadable one, budgets hold
  1  no baseline, a budget broken
  1  no baseline, a policy without budgets (nothing judged)
  1  head unreadable (nothing judged)
Which kind of 1 it was is in the output: a server answer prints the body on stdout
with stderr empty, as on exit 0; an error prints one JSON document on stderr with
stdout empty, as for every command. Reading needs only access to the repository.
Poll with `report` to wait for CI's upload. Same token and base URL environment as `auth`."
    )]
    Diff(DiffArgs),
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
            CloudCommand::Diff(args) => diff::run(output, args),
        }
    }
}
