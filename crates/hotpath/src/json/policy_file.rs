//! Finds and reads the policy file of a checkout: the one lookup the
//! `hotpath-cloud` upload (`lib_on/report_meta.rs`) and `hotpath cloud
//! validate-policy` share, so the CLI can never approve a file the upload
//! would not pick.
//!
//! Order: the file `HOTPATH_POLICY_PATH` names, then
//! `hotpath/<benchmark>-policy.toml`, then `hotpath/policy.toml`, then none.
//! A benchmark without its own file uses the shared one. A file that is not
//! there is absent and the next one is tried; a file that is there but cannot
//! be sent is `PolicyLookup::Unusable`, never treated as absent.
//!
//! The document is sent as written and never parsed here: the server's
//! parser is the only judge.

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use crate::json::cloud_api::POLICY_MAX_BYTES;
use crate::json::JsonPolicy;

/// Directory of the policy files, relative to the repository root.
pub const POLICY_DIR: &str = "hotpath";
const SHARED_POLICY_FILE: &str = "policy.toml";
const BENCHMARK_POLICY_SUFFIX: &str = "-policy.toml";

/// `HOTPATH_POLICY_PATH`: the policy file for this run, instead of the ones
/// in `hotpath/`. Relative to the working directory, or absolute; either way
/// it must resolve inside the repository. Blank or unset names nothing.
pub static POLICY_PATH: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    std::env::var_os("HOTPATH_POLICY_PATH")
        .filter(|path| !path.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
});

#[derive(Debug)]
pub enum PolicyLookup {
    /// The repository has no policy file for the run, or there is no
    /// repository to look in: hotpath.rs refuses the upload.
    NotFound,
    Found(JsonPolicy),
    /// A file is there, or was asked for by name, and cannot be sent.
    Unusable(UnusablePolicy),
}

#[derive(Debug)]
pub struct UnusablePolicy {
    /// The file as a message names it.
    pub file: String,
    pub reason: UnusableReason,
}

#[derive(Debug)]
pub enum UnusableReason {
    Unreadable(std::io::Error),
    NotUtf8(std::string::FromUtf8Error),
    Blank,
    TooLarge,
    /// The file resolves, symlinks followed, to a place outside the
    /// repository, so it has no path a report could name.
    OutsideRepository {
        root: PathBuf,
    },
    /// `HOTPATH_POLICY_PATH` is set and there is no repository to resolve it
    /// inside.
    NoRepository,
}

impl fmt::Display for UnusablePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = &self.file;
        match &self.reason {
            UnusableReason::Unreadable(error) => {
                write!(f, "could not read the policy file {file}: {error}.")
            }
            UnusableReason::NotUtf8(error) => {
                write!(f, "the policy file {file} is not valid UTF-8: {error}.")
            }
            UnusableReason::Blank => write!(f, "the policy file {file} is blank."),
            UnusableReason::TooLarge => write!(
                f,
                "the policy file {file} is larger than {POLICY_MAX_BYTES} bytes, the most the server stores."
            ),
            UnusableReason::OutsideRepository { root } => write!(
                f,
                "the policy file {file} is outside the repository `{}`.",
                root.display()
            ),
            UnusableReason::NoRepository => write!(
                f,
                "the policy file {file} cannot be placed in a repository: no git repository was found."
            ),
        }
    }
}

/// The files a run of `benchmark` would pick up, as a message names them:
/// what to add when there is none.
pub fn policy_files_hint(benchmark: Option<&str>) -> String {
    let shared = format!("`{}`", policy_file_path(None));
    match benchmark {
        Some(_) => format!("`{}` or {shared}", policy_file_path(benchmark)),
        None => shared,
    }
}

/// The policy file of `benchmark` relative to the repository root
/// (`hotpath/<benchmark>-policy.toml`), the shared `hotpath/policy.toml`
/// without one.
pub fn policy_file_path(benchmark: Option<&str>) -> String {
    format!("{POLICY_DIR}/{}", policy_file_name(benchmark))
}

fn policy_file_name(benchmark: Option<&str>) -> String {
    match benchmark {
        Some(name) => format!("{name}{BENCHMARK_POLICY_SUFFIX}"),
        None => SHARED_POLICY_FILE.to_string(),
    }
}

/// Nearest ancestor containing `.git` - a directory for regular checkouts, a
/// `gitdir:` file for worktrees and submodules; `exists()` covers both.
pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The policy a run of `benchmark` in the checkout at `git_root` is judged
/// under.
pub fn lookup(git_root: Option<&Path>, benchmark: Option<&str>) -> PolicyLookup {
    if let Some(path) = POLICY_PATH.as_deref() {
        return match read_override(git_root, path) {
            Ok(policy) => PolicyLookup::Found(policy),
            Err(unusable) => PolicyLookup::Unusable(unusable),
        };
    }
    let Some(git_root) = git_root else {
        return PolicyLookup::NotFound;
    };
    let benchmark_file = benchmark.map(|name| policy_file_name(Some(name)));
    for file_name in benchmark_file.into_iter().chain([policy_file_name(None)]) {
        match read_in_policy_dir(git_root, &file_name) {
            Ok(None) => {}
            Ok(Some(policy)) => return PolicyLookup::Found(policy),
            Err(unusable) => return PolicyLookup::Unusable(unusable),
        }
    }
    PolicyLookup::NotFound
}

/// The document `reader` yields, refused when it is unreadable, over
/// `POLICY_MAX_BYTES`, not UTF-8 or blank. Reads at most one byte past the
/// limit, so an oversized input is never buffered whole. `file` is how the
/// refusal names the input.
pub fn read_source(reader: &mut dyn Read, file: &str) -> Result<String, UnusablePolicy> {
    let unusable = |reason| UnusablePolicy {
        file: file.to_string(),
        reason,
    };
    let mut bytes = Vec::new();
    reader
        .take(POLICY_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| unusable(UnusableReason::Unreadable(error)))?;
    if bytes.len() > POLICY_MAX_BYTES {
        return Err(unusable(UnusableReason::TooLarge));
    }
    let source =
        String::from_utf8(bytes).map_err(|error| unusable(UnusableReason::NotUtf8(error)))?;
    if source.trim().is_empty() {
        return Err(unusable(UnusableReason::Blank));
    }
    Ok(source)
}

/// The document at `path`, wherever it is: a candidate that is not the
/// policy of a checkout yet, so it has no repository path.
pub fn read_file(path: &Path) -> Result<String, UnusablePolicy> {
    let file = format!("`{}`", path.display());
    match File::open(path) {
        Ok(mut reader) => read_source(&mut reader, &file),
        Err(error) => Err(UnusablePolicy {
            file,
            reason: UnusableReason::Unreadable(error),
        }),
    }
}

/// `hotpath/<file_name>` of the checkout; `Ok(None)` when it is not there.
fn read_in_policy_dir(
    git_root: &Path,
    file_name: &str,
) -> Result<Option<JsonPolicy>, UnusablePolicy> {
    let dir = git_root.join(POLICY_DIR);
    // Also covers a `hotpath` that is a file, where looking below it fails
    // with something other than "not found".
    if !dir.is_dir() {
        return Ok(None);
    }
    let path = dir.join(file_name);
    let file = format!("`{POLICY_DIR}/{file_name}`");
    // Not `exists()`: a dangling symlink is a file that is there and cannot
    // be read, not an absent one.
    match path.symlink_metadata() {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(UnusablePolicy {
                file,
                reason: UnusableReason::Unreadable(error),
            })
        }
    }
    read_in_repository(git_root, &path, file).map(Some)
}

/// The file `HOTPATH_POLICY_PATH` names. It was asked for by name, so a
/// missing file is unusable, not absent.
fn read_override(git_root: Option<&Path>, path: &Path) -> Result<JsonPolicy, UnusablePolicy> {
    let file = format!("`{}` (HOTPATH_POLICY_PATH)", path.display());
    let Some(git_root) = git_root else {
        return Err(UnusablePolicy {
            file,
            reason: UnusableReason::NoRepository,
        });
    };
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(error) => {
                return Err(UnusablePolicy {
                    file,
                    reason: UnusableReason::Unreadable(error),
                })
            }
        }
    };
    read_in_repository(git_root, &path, file)
}

/// The document at `path`, named by where it resolves to inside `git_root`.
/// Symlinks are resolved before the check, so a link that leads out of the
/// repository is refused and a link inside it is named by its target.
fn read_in_repository(
    git_root: &Path,
    path: &Path,
    file: String,
) -> Result<JsonPolicy, UnusablePolicy> {
    let unreadable = |error| UnusablePolicy {
        file: file.clone(),
        reason: UnusableReason::Unreadable(error),
    };
    let resolved = path.canonicalize().map_err(unreadable)?;
    let root = git_root.canonicalize().map_err(unreadable)?;
    let Some(relative) = repository_path(&root, &resolved) else {
        return Err(UnusablePolicy {
            file,
            reason: UnusableReason::OutsideRepository {
                root: git_root.to_path_buf(),
            },
        });
    };
    let mut reader = File::open(&resolved).map_err(unreadable)?;
    let source = read_source(&mut reader, &file)?;
    Ok(JsonPolicy {
        source,
        path: relative,
    })
}

/// `resolved` relative to `root` with forward slashes, as a report names it;
/// `None` when it is not below `root`. Both are canonical, so there is no
/// `..` left to check for.
fn repository_path(root: &Path, resolved: &Path) -> Option<String> {
    let relative = resolved.strip_prefix(root).ok()?;
    let segments = relative
        .components()
        .map(|component| match component {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!segments.is_empty()).then(|| segments.join("/"))
}
