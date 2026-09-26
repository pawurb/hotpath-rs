//! Which repository a `hotpath cloud` command targets: `--repo owner/name`
//! when given, else the `origin` remote of the current directory (via `git
//! remote get-url origin`), which must be on github.com. Whichever way it
//! came, the name is validated against GitHub's own character set before it
//! is put in a request path, and a bad `--repo` never falls back to the
//! remote. A remote URL quoted in an error has its userinfo removed: an
//! `https://user:token@host/...` origin must not land in captured logs.

use std::process::Command;

use hotpath::json::cloud_api::repository_from_remote_url;

use crate::cmd::cloud::api::CliError;

/// `owner/name`, validated: exactly two non-empty segments of
/// `[A-Za-z0-9._-]`, neither `.` nor `..`. Safe to interpolate into a request
/// path without percent-encoding.
pub(crate) fn resolve(flag: Option<&str>) -> Result<String, CliError> {
    match flag {
        Some(repo) => validate(repo).ok_or_else(|| {
            CliError::client(format!(
                "invalid --repo `{repo}`: expected owner/name, each made of letters, digits, `.`, `_` and `-`."
            ))
        }),
        None => from_origin(),
    }
}

fn from_origin() -> Result<String, CliError> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| {
            CliError::client(format!("could not run git ({e}); pass --repo owner/name."))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.lines().next().unwrap_or("").trim();
        return Err(CliError::client(format!(
            "no origin remote in the current directory ({reason}); pass --repo owner/name."
        )));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let repo = repository_from_remote_url(&url).ok_or_else(|| {
        CliError::client(format!(
            "the origin remote `{}` is not on github.com; pass --repo owner/name.",
            redact_userinfo(&url)
        ))
    })?;
    validate(&repo).ok_or_else(|| {
        CliError::client(format!(
            "the origin remote `{}` does not name a GitHub owner/name; pass --repo owner/name.",
            redact_userinfo(&url)
        ))
    })
}

/// The URL without its `user[:password]@` part, for quoting in a message.
/// Only the `scheme://` form carries one; the scp-like `git@host:path` user
/// is a login, not a credential, and stays.
fn redact_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if path.is_empty() {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}/{path}")
    }
}

fn validate(repo: &str) -> Option<String> {
    let (owner, name) = repo.split_once('/')?;
    if [owner, name].into_iter().all(valid_segment) {
        Some(repo.to_string())
    } else {
        None
    }
}

fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
