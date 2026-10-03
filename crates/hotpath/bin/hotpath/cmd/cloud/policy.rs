//! `hotpath cloud validate-policy`: asks the server whether a policy file
//! parses. Without `--file` the file is the one
//! `hotpath::json::policy_file::lookup` picks, the same lookup the upload uses.
//! The document is sent as written and never parsed here: the server's parser
//! is the only judge. Local problems (unreadable, not UTF-8, blank, too large,
//! outside the repository) are reported without a request. The route is
//! public, so the request is anonymous.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::{ApiErrorCode, PolicyProblem, PolicyRejected, PolicyValidation};
use hotpath::json::policy_file::{self, find_git_root, PolicyLookup, UnusablePolicy, POLICY_DIR};
use hotpath::json::JsonPolicy;
use serde::de::IgnoredAny;
use serde::Serialize;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

const VALIDATE_PATH: &str = "/api/v1/policy/validate";

#[derive(Args, Debug)]
pub(crate) struct ValidatePolicyArgs {
    #[arg(
        long,
        value_name = "NAME",
        conflicts_with = "file",
        help = "Check the policy file a run of this benchmark is judged under"
    )]
    benchmark: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Check this file instead of the one the repository picks, `-` for stdin"
    )]
    file: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct CheckedFile {
    /// Relative to the repository root for a file of the repository, as
    /// given for `--file`.
    path: String,
    valid: bool,
    /// Empty exactly when `valid`.
    problems: Vec<PolicyProblem>,
}

/// A file to check: read, or refused before any request.
struct Candidate {
    path: String,
    source: Result<String, UnusablePolicy>,
}

impl Candidate {
    fn found(policy: JsonPolicy) -> Self {
        Self {
            path: policy.path,
            source: Ok(policy.source),
        }
    }

    /// A file that is there and cannot be sent. It is named as the refusal
    /// names it, since it may have no path inside the repository.
    fn unusable(unusable: UnusablePolicy) -> Self {
        Self {
            path: unusable.file.trim_matches('`').to_string(),
            source: Err(unusable),
        }
    }
}

pub(crate) fn validate(output: &Output, args: ValidatePolicyArgs) -> Result<ExitCode, CliError> {
    let candidate = candidate(&args)?;

    let problems = match candidate.source {
        Ok(source) => server_problems(&Client::anonymous(), source)?,
        Err(unusable) => vec![PolicyProblem {
            line: None,
            message: unusable.to_string(),
        }],
    };
    let checked = CheckedFile {
        path: candidate.path,
        valid: problems.is_empty(),
        problems,
    };

    output.emit(&checked)?;
    Ok(if checked.valid {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The one file the arguments name: no file to check is an error.
fn candidate(args: &ValidatePolicyArgs) -> Result<Candidate, CliError> {
    if let Some(file) = &args.file {
        let source = if file.as_os_str() == "-" {
            policy_file::read_source(&mut std::io::stdin().lock(), "stdin (--file -)")
        } else {
            policy_file::read_file(file)
        };
        return Ok(Candidate {
            path: file.display().to_string(),
            source,
        });
    }

    let benchmark = args
        .benchmark
        .as_deref()
        .map(repo::validate_benchmark)
        .transpose()?;
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::client(format!("could not read the working directory: {e}")))?;
    let git_root = find_git_root(&cwd);

    match policy_file::lookup(git_root.as_deref(), benchmark) {
        PolicyLookup::Found(policy) => Ok(Candidate::found(policy)),
        PolicyLookup::Unusable(unusable) => Ok(Candidate::unusable(unusable)),
        PolicyLookup::NotFound => Err(CliError::client(match (&git_root, benchmark) {
            (None, _) => not_a_repository(&cwd),
            (Some(root), Some(benchmark)) => format!(
                "no policy file for benchmark `{benchmark}` in `{}`: neither `{POLICY_DIR}/{benchmark}-policy.toml` nor `{POLICY_DIR}/policy.toml` exists, so hotpath.rs refuses its reports. Add one of them.",
                root.display()
            ),
            (Some(root), None) => format!(
                "no shared policy file in `{}`: `{POLICY_DIR}/policy.toml` does not exist, so hotpath.rs refuses the reports of every benchmark without its own policy file. Check a benchmark's own `{POLICY_DIR}/<benchmark>-policy.toml` with --benchmark NAME.",
                root.display()
            ),
        })),
    }
}

fn not_a_repository(cwd: &Path) -> String {
    format!(
        "`{}` is not inside a git repository, so it has no policy files and hotpath.rs refuses its reports. Use --file to check one file.",
        cwd.display()
    )
}

/// What the server finds wrong with `source`, empty when it is a policy (the
/// 200 body is not read). Only an `InvalidPolicy` rejection is an answer;
/// anything else is a command failure.
fn server_problems(client: &Client, source: String) -> Result<Vec<PolicyProblem>, CliError> {
    match client.post::<_, IgnoredAny>(VALIDATE_PATH, &PolicyValidation { source }) {
        Ok(_) => Ok(Vec::new()),
        Err(CliError::Server(body)) => match serde_json::from_str::<PolicyRejected>(&body) {
            Ok(rejected) if rejected.code == ApiErrorCode::InvalidPolicy => Ok(rejected.problems),
            _ => Err(CliError::Server(body)),
        },
        Err(error) => Err(error),
    }
}
