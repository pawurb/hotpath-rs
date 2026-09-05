//! Builds the report `meta` object: toolchain, OS, timestamp, the source-root
//! prefix the server needs to map relative `location.file` values back to
//! repository paths, and - with the `hotpath-cloud` feature - the git/CI
//! provenance that makes a report self-describing.

use std::path::{Path, PathBuf};

pub(crate) fn build_meta() -> crate::json::JsonMeta {
    let source_root = source_root();

    // Fail loudly on upload runs: broken source links would otherwise only
    // surface as dead links on the server.
    #[cfg(feature = "hotpath-cloud")]
    if source_root.is_none()
        && crate::lib_on::cloud::enabled()
        && crate::lib_on::locations::any_relative_file().is_some()
    {
        eprintln!(
            "hotpath: could not resolve the source checkout from the working directory; \
             set HOTPATH_SOURCE_ROOT to the workspace path relative to the repo root \
             (source links will be unavailable in this report)"
        );
    }

    cfg_if::cfg_if! {
        if #[cfg(feature = "hotpath-cloud")] {
            let ci = crate::lib_on::ci_info::detect();
            let git = merge_git_info(local_git_info(), ci.as_ref());
            // Set on every report, not only uploads, so a saved JSON says
            // which benchmark it is; the upload path reports invalid names.
            let benchmark = crate::lib_on::cloud::benchmark_name().ok();
            let ci = ci.map(|ci| ci.ci);
        } else {
            let git: Option<crate::json::JsonGitInfo> = None;
            let ci: Option<crate::json::JsonCiInfo> = None;
            let benchmark: Option<String> = None;
        }
    }

    crate::json::JsonMeta {
        rustc: env!("HOTPATH_RUSTC_VERSION").to_string(),
        os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        created_at: format_rfc3339_utc(std::time::SystemTime::now()),
        source_root,
        git,
        ci,
        benchmark,
    }
}

#[cfg(feature = "hotpath-cloud")]
fn local_git_info() -> Option<crate::json::JsonGitInfo> {
    // A `HOTPATH_SOURCE_ROOT` override asserts the runtime checkout is the
    // source checkout, so its git identity is trusted even when checkout
    // resolution fails.
    let git_root = if std::env::var("HOTPATH_SOURCE_ROOT").is_ok() {
        find_git_root(&std::env::current_dir().ok()?)?
    } else {
        resolve_checkout()?.git_root
    };
    crate::lib_on::git_info::read_git_info_at(&git_root)
}

/// CI values win over the checkout: on a pull request run the runner checks
/// out `refs/pull/<n>/merge` detached, so `.git` reports no ref at all and a
/// sha that only the provider can name. The checkout still fills the gaps a
/// provider leaves.
#[cfg(feature = "hotpath-cloud")]
fn merge_git_info(
    local: Option<crate::json::JsonGitInfo>,
    ci: Option<&crate::lib_on::ci_info::CiContext>,
) -> Option<crate::json::JsonGitInfo> {
    let Some(ci) = ci else {
        return local;
    };
    let (sha, r#ref, base_sha, repository) = match local {
        Some(git) => (Some(git.sha), git.r#ref, git.base_sha, git.repository),
        None => (None, None, None, None),
    };
    Some(crate::json::JsonGitInfo {
        sha: ci.sha.clone().or(sha)?,
        r#ref: ci.r#ref.clone().or(r#ref),
        base_sha: ci.base_sha.clone().or(base_sha),
        repository: ci.repository.clone().or(repository),
    })
}

/// Build workspace root relative to the enclosing git root: the prefix to
/// prepend to relative `location.file` values ("" when the workspace root is
/// the repo root). `HOTPATH_SOURCE_ROOT` overrides; `None` when the checkout
/// cannot be resolved - an unverified value would produce broken links, so
/// the field is omitted instead of guessed.
fn source_root() -> Option<String> {
    if let Ok(v) = std::env::var("HOTPATH_SOURCE_ROOT") {
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
/// outside the checkout, deleted sources); `HOTPATH_SOURCE_ROOT` is the
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

    #[cfg(feature = "hotpath-cloud")]
    mod cloud {
        use crate::json::{JsonCiInfo, JsonGitInfo, JsonMeta};
        use crate::lib_on::ci_info::CiContext;
        use crate::lib_on::report_meta::merge_git_info;

        const LOCAL_SHA: &str = "1111111111111111111111111111111111111111";
        const CI_SHA: &str = "2222222222222222222222222222222222222222";
        const BASE_SHA: &str = "3333333333333333333333333333333333333333";

        fn detached_local() -> JsonGitInfo {
            JsonGitInfo {
                sha: LOCAL_SHA.to_string(),
                r#ref: None,
                base_sha: None,
                repository: Some("pawurb/hotpath-rs".to_string()),
            }
        }

        fn pull_request_ci() -> CiContext {
            CiContext {
                ci: JsonCiInfo {
                    provider: "github-actions".to_string(),
                    event: Some("pull_request".to_string()),
                    pr_number: Some(42),
                    ..JsonCiInfo::default()
                },
                sha: Some(CI_SHA.to_string()),
                r#ref: Some("refs/pull/42/merge".to_string()),
                repository: Some("pawurb/hotpath-rs".to_string()),
                base_sha: Some(BASE_SHA.to_string()),
            }
        }

        #[test]
        fn ci_values_win_over_the_checkout() {
            let git =
                merge_git_info(Some(detached_local()), Some(&pull_request_ci())).expect("git info");
            assert_eq!(git.sha, CI_SHA);
            assert_eq!(git.r#ref.as_deref(), Some("refs/pull/42/merge"));
            assert_eq!(git.base_sha.as_deref(), Some(BASE_SHA));
        }

        #[test]
        fn checkout_fills_gaps_the_provider_leaves() {
            let bare_ci = CiContext {
                ci: JsonCiInfo {
                    provider: "github-actions".to_string(),
                    ..JsonCiInfo::default()
                },
                sha: None,
                r#ref: None,
                repository: None,
                base_sha: None,
            };
            let local = JsonGitInfo {
                r#ref: Some("refs/heads/main".to_string()),
                ..detached_local()
            };
            let git = merge_git_info(Some(local), Some(&bare_ci)).expect("git info");
            assert_eq!(git.sha, LOCAL_SHA);
            assert_eq!(git.r#ref.as_deref(), Some("refs/heads/main"));
            assert_eq!(git.repository.as_deref(), Some("pawurb/hotpath-rs"));
        }

        #[test]
        fn ci_alone_still_yields_git_info() {
            let git = merge_git_info(None, Some(&pull_request_ci())).expect("git info");
            assert_eq!(git.sha, CI_SHA);
            assert_eq!(git.repository.as_deref(), Some("pawurb/hotpath-rs"));

            assert!(merge_git_info(None, None).is_none());
        }

        /// An old consumer must see no new keys.
        #[test]
        fn absent_fields_add_no_keys() {
            let meta = JsonMeta {
                rustc: "1.89.0".to_string(),
                os: "macos-aarch64".to_string(),
                created_at: "2026-08-27T10:15:42Z".to_string(),
                source_root: Some(String::new()),
                git: Some(detached_local()),
                ci: None,
                benchmark: None,
            };
            let value: serde_json::Value = serde_json::to_value(&meta).unwrap();
            let keys = |value: &serde_json::Value| -> Vec<String> {
                value.as_object().unwrap().keys().cloned().collect()
            };
            assert_eq!(
                keys(&value),
                ["created_at", "git", "os", "rustc", "source_root"]
            );
            assert_eq!(keys(&value["git"]), ["repository", "sha"]);
        }

        #[test]
        fn new_fields_round_trip() {
            let meta = JsonMeta {
                rustc: "1.89.0".to_string(),
                os: "macos-aarch64".to_string(),
                created_at: "2026-08-27T10:15:42Z".to_string(),
                source_root: Some(String::new()),
                git: merge_git_info(Some(detached_local()), Some(&pull_request_ci())),
                ci: Some(pull_request_ci().ci),
                benchmark: Some("ci".to_string()),
            };
            let json = serde_json::to_string(&meta).unwrap();
            let back: JsonMeta = serde_json::from_str(&json).unwrap();

            let git = back.git.expect("git info");
            assert_eq!(git.sha, CI_SHA);
            assert_eq!(git.base_sha.as_deref(), Some(BASE_SHA));
            assert_eq!(git.repository.as_deref(), Some("pawurb/hotpath-rs"));
            let ci = back.ci.expect("ci info");
            assert_eq!(ci.provider, "github-actions");
            assert_eq!(ci.event.as_deref(), Some("pull_request"));
            assert_eq!(ci.pr_number, Some(42));
            assert_eq!(back.benchmark.as_deref(), Some("ci"));
        }

        #[test]
        fn old_report_deserializes() {
            let json = r#"{
                "rustc": "1.89.0",
                "os": "macos-aarch64",
                "created_at": "2026-08-27T10:15:42Z",
                "source_root": "",
                "git": {"sha": "1111111111111111111111111111111111111111", "ref": "refs/heads/main"}
            }"#;
            let meta: JsonMeta = serde_json::from_str(json).unwrap();
            let git = meta.git.expect("git info");
            assert_eq!(git.base_sha, None);
            assert_eq!(git.repository, None);
            assert!(meta.ci.is_none());
            assert!(meta.benchmark.is_none());
        }
    }

    #[test]
    fn rfc3339_formatting() {
        let t = UNIX_EPOCH + Duration::from_secs(20_692 * 86_400 + 10 * 3_600 + 15 * 60 + 42);
        assert_eq!(format_rfc3339_utc(t), "2026-08-27T10:15:42Z");
    }
}
