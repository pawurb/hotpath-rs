//! Builds the report `meta` object: toolchain, OS, timestamp, and the
//! source-root prefix the server needs to map relative `location.file` values
//! back to repository paths.

use std::path::{Path, PathBuf};

pub(crate) fn build_meta() -> crate::json::JsonMeta {
    let source_root = source_root();

    // Fail loudly on upload runs: broken source links would otherwise only
    // surface as dead links on the server.
    #[cfg(feature = "hotpath-cloud-meta")]
    if source_root.is_none()
        && crate::lib_on::cloud::enabled()
        && crate::lib_on::locations::any_relative_file().is_some()
    {
        eprintln!(
            "hotpath: could not resolve the source checkout from the working directory; \
             set HOTPATH_META_SOURCE_ROOT to the workspace path relative to the repo root \
             (source links will be unavailable in this report)"
        );
    }

    crate::json::JsonMeta {
        rustc: env!("HOTPATH_META_RUSTC_VERSION").to_string(),
        os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        created_at: format_rfc3339_utc(std::time::SystemTime::now()),
        source_root,
        git: git_info(),
    }
}

fn git_info() -> Option<crate::json::JsonGitInfo> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-cloud-meta")] {
            // A `HOTPATH_META_SOURCE_ROOT` override asserts the runtime
            // checkout is the source checkout, so its git identity is trusted
            // even when checkout resolution fails.
            let git_root = if std::env::var("HOTPATH_META_SOURCE_ROOT").is_ok() {
                let cwd = std::env::current_dir().ok()?;
                find_git_root(&cwd)?
            } else {
                resolve_checkout()?.git_root
            };
            crate::lib_on::git_info::read_git_info_at(&git_root)
        } else {
            None
        }
    }
}

/// Build workspace root relative to the enclosing git root: the prefix to
/// prepend to relative `location.file` values ("" when the workspace root is
/// the repo root). `HOTPATH_META_SOURCE_ROOT` overrides; `None` when the
/// checkout cannot be resolved - an unverified value would produce broken
/// links, so the field is omitted instead of guessed.
fn source_root() -> Option<String> {
    if let Ok(v) = std::env::var("HOTPATH_META_SOURCE_ROOT") {
        return Some(v);
    }
    let checkout = resolve_checkout()?;
    let rel = checkout
        .workspace_root
        .strip_prefix(&checkout.git_root)
        .ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// The checkout the report's relative `location.file` values were compiled
/// from, verified against the filesystem rather than guessed from the
/// runtime working directory.
struct ResolvedCheckout {
    git_root: PathBuf,
    workspace_root: PathBuf,
}

/// Relative `file!()` paths are relative to the directory cargo invoked rustc
/// from (the build workspace root), not the runtime working directory. The
/// workspace root is located as the nearest ancestor of the working directory
/// that actually contains one of the registered relative source files, and
/// the git root as its enclosing checkout - so both are verified to belong to
/// the sources in the report. `None` when nothing relative is registered or
/// no ancestor matches (a nested workspace launched from above it, running
/// outside the checkout, deleted sources); `HOTPATH_META_SOURCE_ROOT` is the
/// escape hatch for those layouts.
fn resolve_checkout() -> Option<ResolvedCheckout> {
    let cwd = std::env::current_dir().ok()?;
    let probe = crate::lib_on::locations::any_relative_file()?;
    let workspace_root = cwd
        .ancestors()
        .find(|dir| dir.join(probe).is_file())
        .map(Path::to_path_buf)?;
    let git_root = find_git_root(&workspace_root)?;
    Some(ResolvedCheckout {
        git_root,
        workspace_root,
    })
}

/// Nearest ancestor containing `.git` - a directory for regular checkouts, a
/// `gitdir:` file for worktrees and submodules; `exists()` covers both.
pub(crate) fn find_git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

fn format_rfc3339_utc(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Days since the Unix epoch to (year, month, day) in the proleptic Gregorian
/// calendar (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use crate::lib_on::report_meta::{civil_from_days, format_rfc3339_utc};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        // 2024 is a leap year.
        assert_eq!(civil_from_days(19_723 + 59), (2024, 2, 29));
        assert_eq!(civil_from_days(20_692), (2026, 8, 27));
    }

    #[test]
    fn rfc3339_formatting() {
        let t = UNIX_EPOCH + Duration::from_secs(20_692 * 86_400 + 10 * 3_600 + 15 * 60 + 42);
        assert_eq!(format_rfc3339_utc(t), "2026-08-27T10:15:42Z");
    }
}
