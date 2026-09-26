//! `hotpath cloud get-policy` / `set-policy`: read and replace the PR comment
//! policy of a repository (`--repo`) or of one of its benchmarks
//! (`--benchmark`), the TOML document the dashboard's policy editor edits.
//! The loop they serve: `get-policy`, edit, `set-policy --dry-run` until the
//! server finds no problem (a refusal lists every problem it found, see
//! `PolicyRejected`), then `set-policy`.
//!
//! The document is sent as written and never parsed here: the server's
//! parser is the only judge, a second one would disagree with it on some
//! input. The client refuses only what the server would refuse for certain
//! without reading it (unreadable, not UTF-8, blank, too large), before any
//! request. Every argument is validated before the client is built, so a bad
//! value is reported without a token and never costs a request.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::{PolicySaved, PolicyUpdate, PolicyView, POLICY_MAX_BYTES};

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

/// The policy a command reads or writes: the repository's, or one
/// benchmark's when `--benchmark` is given.
#[derive(Args, Debug)]
pub(crate) struct PolicyScope {
    #[arg(long, value_name = "OWNER/NAME", help = "The repository")]
    repo: String,

    #[arg(
        long,
        value_name = "NAME",
        help = "One benchmark's policy instead of the repository's"
    )]
    benchmark: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct SetPolicyArgs {
    #[command(flatten)]
    scope: PolicyScope,

    #[arg(
        long,
        value_name = "PATH",
        help = "The TOML policy document to store, `-` for stdin"
    )]
    file: PathBuf,

    #[arg(
        long,
        help = "Validate only: the server checks the policy but stores nothing"
    )]
    dry_run: bool,
}

pub(crate) fn get(output: &Output, scope: PolicyScope) -> Result<ExitCode, CliError> {
    let path = request_path(&scope)?;
    let view: PolicyView = Client::from_env()?.get(&path)?;
    output.emit(&view)?;
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn set(output: &Output, args: SetPolicyArgs) -> Result<ExitCode, CliError> {
    let path = request_path(&args.scope)?;
    let update = PolicyUpdate {
        source: read_source(&args.file)?,
        dry_run: args.dry_run,
    };
    let saved: PolicySaved = Client::from_env()?.put(&path, &update)?;
    output.emit(&saved)?;
    Ok(ExitCode::SUCCESS)
}

/// `.../policy` of the repository, or of the benchmark when one is named.
fn request_path(scope: &PolicyScope) -> Result<String, CliError> {
    let repo = repo::validate(&scope.repo)?;
    Ok(match &scope.benchmark {
        None => format!("/api/v1/repos/{repo}/policy"),
        Some(benchmark) => {
            let benchmark = repo::validate_benchmark(benchmark)?;
            format!("/api/v1/repos/{repo}/benchmarks/{benchmark}/policy")
        }
    })
}

/// The document at `file` (`-` is stdin), refused when it is unreadable, over
/// `POLICY_MAX_BYTES`, not UTF-8 or blank. Reads at most one byte past the
/// limit, so an oversized input is never buffered whole.
fn read_source(file: &Path) -> Result<String, CliError> {
    let from_stdin = file.as_os_str() == "-";
    let name = if from_stdin {
        "stdin (--file -)".to_string()
    } else {
        format!("`{}`", file.display())
    };
    let read_capped = |reader: &mut dyn Read| {
        let mut bytes = Vec::new();
        reader
            .take(POLICY_MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    };
    let bytes = if from_stdin {
        read_capped(&mut std::io::stdin().lock())
    } else {
        File::open(file).and_then(|mut f| read_capped(&mut f))
    }
    .map_err(|e| CliError::client(format!("could not read the policy file {name}: {e}")))?;

    if bytes.len() > POLICY_MAX_BYTES {
        return Err(CliError::client(format!(
            "the policy file {name} is larger than {POLICY_MAX_BYTES} bytes, the most the server stores."
        )));
    }
    let source = String::from_utf8(bytes)
        .map_err(|e| CliError::client(format!("the policy file {name} is not valid UTF-8: {e}")))?;
    if source.trim().is_empty() {
        return Err(CliError::client(format!(
            "the policy file {name} is blank."
        )));
    }
    Ok(source)
}
