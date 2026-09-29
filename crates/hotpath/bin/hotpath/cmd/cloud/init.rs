//! `hotpath cloud init`: writes the server's key defaults
//! (`GET /api/v1/policy/default`) as a starting policy file of this
//! repository, so a repository gets the file every upload must carry without
//! copying one by hand. The document comes from the server on every run, so
//! the client never carries a copy of the defaults that could drift from the
//! release that judges.
//!
//! The file is written byte for byte, comments included: they document each
//! key. An existing file is user configuration and is never replaced without
//! `--force`, and a symlink is never followed. Every refusal is decided
//! before the client is built, so it needs no token and costs no request.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use hotpath::json::cloud_api::DefaultPolicy;
use hotpath::json::policy_file::{find_git_root, policy_file_path};
use serde::Serialize;

use crate::cmd::cloud::api::{CliError, Client, Output};
use crate::cmd::cloud::repo;

const DEFAULT_POLICY_PATH: &str = "/api/v1/policy/default";

#[derive(Args, Debug)]
pub(crate) struct InitArgs {
    #[arg(
        long,
        value_name = "NAME",
        help = "Write this benchmark's own policy file instead of the shared one"
    )]
    benchmark: Option<String>,

    #[arg(long, help = "Replace the policy file when it already exists")]
    force: bool,
}

#[derive(Debug, Serialize)]
struct WrittenFile {
    /// Relative to the repository root.
    path: String,
    /// An existing file was replaced (`--force`).
    replaced: bool,
}

pub(crate) fn run(output: &Output, args: InitArgs) -> Result<ExitCode, CliError> {
    let benchmark = args
        .benchmark
        .as_deref()
        .map(repo::validate_benchmark)
        .transpose()?;
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::client(format!("could not read the working directory: {e}")))?;
    let root = find_git_root(&cwd).ok_or_else(|| {
        CliError::client(format!(
            "`{}` is not inside a git repository, so there is no repository to write a policy file to.",
            cwd.display()
        ))
    })?;
    let relative = policy_file_path(benchmark);
    let target = root.join(&relative);
    let replaced = existing(&target, &relative, args.force)?;
    if let Some(file) = &output.file {
        if resolve(&cwd.join(file)) == resolve(&target) {
            return Err(CliError::client(format!(
                "--output names `{relative}`, the policy file this command writes; the JSON result would replace it. Drop --output or name another file."
            )));
        }
    }

    let policy: DefaultPolicy = Client::from_env()?.get(DEFAULT_POLICY_PATH)?;
    write(&target, &relative, &policy.source, args.force)?;

    output.emit(&WrittenFile {
        path: relative,
        replaced,
    })?;
    Ok(ExitCode::SUCCESS)
}

/// `path` with `.` and `..` resolved and its deepest existing ancestor
/// canonicalized (symlinks followed), so two spellings of one file compare
/// equal even when the file and its directory do not exist yet.
fn resolve(path: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other),
        }
    }
    let mut missing = Vec::new();
    let mut existing = lexical.as_path();
    loop {
        if let Ok(canonical) = existing.canonicalize() {
            return missing
                .iter()
                .rev()
                .fold(canonical, |path, name| path.join(name));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                existing = parent;
            }
            _ => return lexical,
        }
    }
}

/// Whether `target` is there to be replaced. Anything already at the path is
/// refused without `--force`, and a symlink always: writing through it could
/// land outside the repository.
fn existing(target: &Path, relative: &str, force: bool) -> Result<bool, CliError> {
    match target.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(CliError::client(format!(
            "could not read `{relative}`: {error}"
        ))),
        Ok(meta) if meta.file_type().is_symlink() => Err(CliError::client(format!(
            "`{relative}` is a symlink; replace it by hand."
        ))),
        Ok(meta) if !meta.is_file() => Err(CliError::client(format!(
            "`{relative}` exists and is not a file."
        ))),
        Ok(_) if !force => Err(CliError::client(format!(
            "`{relative}` already exists. Check it with `hotpath cloud validate-policy`, or pass --force to replace it with the defaults."
        ))),
        Ok(_) => Ok(true),
    }
}

/// Writes `source` to `target` whole or not at all: the document goes to a
/// temporary file next to it first, so a failed write never leaves a partial
/// policy file behind (which the next run would refuse to replace). Without
/// `--force` it is linked in only if `target` is still absent, so a file that
/// appeared during the request is not replaced either; with it, it is renamed
/// over the old one.
fn write(target: &Path, relative: &str, source: &str, force: bool) -> Result<(), CliError> {
    let fail = |error: std::io::Error| {
        let error = if error.kind() == std::io::ErrorKind::AlreadyExists {
            "it appeared while the defaults were fetched".to_string()
        } else {
            error.to_string()
        };
        CliError::client(format!("could not write `{relative}`: {error}"))
    };
    let dir = target.parent().expect("a policy file path has a directory");
    std::fs::create_dir_all(dir).map_err(fail)?;
    let temp = TempFile::create(dir, source).map_err(fail)?;
    if force {
        std::fs::rename(&temp.0, target).map_err(fail)
    } else {
        std::fs::hard_link(&temp.0, target).map_err(fail)
    }
}

/// A file removed when dropped, also on an early return or a panic; after a
/// rename there is nothing left to remove.
struct TempFile(PathBuf);

impl TempFile {
    fn create(dir: &Path, source: &str) -> std::io::Result<Self> {
        let temp = Self(dir.join(format!(".hotpath-init-{}.tmp", std::process::id())));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp.0)?;
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        Ok(temp)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
