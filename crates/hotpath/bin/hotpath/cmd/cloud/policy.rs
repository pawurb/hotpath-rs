//! `hotpath cloud validate-policy`: asks the server whether the policy files
//! of this checkout parse as policies, before a run is judged under them.
//! Which file a run picks is decided by `hotpath::json::policy_file`, the
//! lookup the upload uses, so this command never approves a file the upload
//! would not send.
//!
//! The document is sent as written and never parsed here: the server's
//! parser is the only judge. What the client can tell without it (a file
//! that is unreadable, not UTF-8, blank, too large or outside the
//! repository) is reported as a problem of that file and costs no request.
//! Every argument is validated and every file read before the client is
//! built.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::{
    ApiErrorCode, PolicyProblem, PolicyRejected, PolicyValidated, PolicyValidation,
};
use hotpath::json::policy_file::{self, find_git_root, PolicyLookup, UnusablePolicy, POLICY_DIR};
use hotpath::json::JsonPolicy;
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
        help = "Check this file instead of the ones in the repository, `-` for stdin"
    )]
    file: Option<PathBuf>,
}

/// One file of the answer.
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
    let candidates = candidates(&args)?;

    // Built for the first document there is to send, so files that are all
    // refused here are reported without a token.
    let mut client = None;
    let mut checked = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let problems = match candidate.source {
            Ok(source) => {
                let client = match &client {
                    Some(client) => client,
                    None => client.insert(Client::from_env()?),
                };
                server_problems(client, source)?
            }
            Err(unusable) => vec![PolicyProblem {
                line: None,
                message: unusable.to_string(),
            }],
        };
        checked.push(CheckedFile {
            path: candidate.path,
            valid: problems.is_empty(),
            problems,
        });
    }

    output.emit(&checked)?;
    Ok(if checked.iter().all(|file| file.valid) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The files the arguments name, never empty: no file to check is an error.
fn candidates(args: &ValidatePolicyArgs) -> Result<Vec<Candidate>, CliError> {
    if let Some(file) = &args.file {
        let source = if file.as_os_str() == "-" {
            policy_file::read_source(&mut std::io::stdin().lock(), "stdin (--file -)")
        } else {
            policy_file::read_file(file)
        };
        return Ok(vec![Candidate {
            path: file.display().to_string(),
            source,
        }]);
    }

    let benchmark = args
        .benchmark
        .as_deref()
        .map(repo::validate_benchmark)
        .transpose()?;
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::client(format!("could not read the working directory: {e}")))?;
    let git_root = find_git_root(&cwd);

    if let Some(benchmark) = benchmark {
        return match policy_file::lookup(git_root.as_deref(), Some(benchmark)) {
            PolicyLookup::Found(policy) => Ok(vec![Candidate::found(policy)]),
            PolicyLookup::Unusable(unusable) => Ok(vec![Candidate::unusable(unusable)]),
            PolicyLookup::NotFound => Err(CliError::client(match &git_root {
                Some(root) => format!(
                    "no policy file for benchmark `{benchmark}` in `{}`: neither `{POLICY_DIR}/{benchmark}-policy.toml` nor `{POLICY_DIR}/policy.toml` exists, so the built-in default judges it.",
                    root.display()
                ),
                None => not_a_repository(&cwd),
            })),
        };
    }

    let Some(git_root) = git_root else {
        return Err(CliError::client(not_a_repository(&cwd)));
    };
    let files = policy_file::list(&git_root).map_err(|e| CliError::client(e.to_string()))?;
    if files.is_empty() {
        return Err(CliError::client(format!(
            "no policy file in `{}`: neither `{POLICY_DIR}/policy.toml` nor a `{POLICY_DIR}/*-policy.toml` exists, so the built-in default judges every benchmark.",
            git_root.display()
        )));
    }
    Ok(files
        .into_iter()
        .map(|file| match file {
            Ok(policy) => Candidate::found(policy),
            Err(unusable) => Candidate::unusable(unusable),
        })
        .collect())
}

fn not_a_repository(cwd: &std::path::Path) -> String {
    format!(
        "`{}` is not inside a git repository, so it has no policy files. Use --file to check one file.",
        cwd.display()
    )
}

/// What the server finds wrong with `source`, nothing when it is a policy.
/// A refused document is an answer about that file; anything else the server
/// or the network does is a failure of the command.
fn server_problems(client: &Client, source: String) -> Result<Vec<PolicyProblem>, CliError> {
    match client.post::<_, PolicyValidated>(VALIDATE_PATH, &PolicyValidation { source }) {
        Ok(_) => Ok(Vec::new()),
        Err(CliError::Server(body)) => match serde_json::from_str::<PolicyRejected>(&body) {
            Ok(rejected) if rejected.code == ApiErrorCode::InvalidPolicy => Ok(rejected.problems),
            _ => Err(CliError::Server(body)),
        },
        Err(error) => Err(error),
    }
}
