//! The `--repo owner/name` value of the `hotpath cloud` commands that target
//! one repository, validated against GitHub's own character set before it is
//! put in a request path. There is no fallback to the git remote: the caller
//! names the repository, `hotpath cloud repos` lists the choices.

use crate::cmd::cloud::api::CliError;

/// `owner/name`, validated: exactly two non-empty segments of
/// `[A-Za-z0-9._-]`, neither `.` nor `..`. Safe to interpolate into a request
/// path without percent-encoding.
pub(crate) fn validate(repo: &str) -> Result<&str, CliError> {
    let valid = repo
        .split_once('/')
        .is_some_and(|(owner, name)| valid_segment(owner) && valid_segment(name));
    if valid {
        Ok(repo)
    } else {
        Err(CliError::client(format!(
            "invalid --repo `{repo}`: expected owner/name, each made of letters, digits, `.`, `_` and `-`."
        )))
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
