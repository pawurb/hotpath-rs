//! Builds the report `meta` object: toolchain, OS, timestamp, and the
//! source-root prefix the server needs to map relative `location.file` values
//! back to repository paths.

use std::path::{Path, PathBuf};

pub(crate) fn build_meta() -> crate::json::JsonMeta {
    crate::json::JsonMeta {
        rustc: env!("HOTPATH_RUSTC_VERSION").to_string(),
        os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        created_at: format_rfc3339_utc(std::time::SystemTime::now()),
        source_root: source_root(),
        git: git_info(),
    }
}

fn git_info() -> Option<crate::json::JsonGitInfo> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-cloud")] {
            let cwd = std::env::current_dir().ok()?;
            let root = find_git_root(&cwd)?;
            crate::lib_on::git_info::read_git_info_at(&root)
        } else {
            None
        }
    }
}

/// Current directory relative to the enclosing git root: the prefix to
/// prepend to relative `location.file` values ("" when the process runs from
/// the repo root). `HOTPATH_SOURCE_ROOT` overrides (for CI jobs using
/// `working-directory:`); `None` when no git root is found.
fn source_root() -> Option<String> {
    if let Ok(v) = std::env::var("HOTPATH_SOURCE_ROOT") {
        return Some(v);
    }
    let cwd = std::env::current_dir().ok()?;
    let root = find_git_root(&cwd)?;
    let rel = cwd.strip_prefix(&root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
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
