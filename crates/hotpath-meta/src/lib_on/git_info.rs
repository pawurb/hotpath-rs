//! Reads `sha` and `ref` for the report `meta.git` object straight from the
//! `.git` directory - never shells out to git. In CI the server trusts OIDC
//! claims over this data; it mainly serves local repo-token uploads and
//! offline reports.

use std::path::{Path, PathBuf};

pub(crate) fn read_git_info_at(root: &Path) -> Option<crate::json::JsonGitInfo> {
    let git_dir = resolve_git_dir(root)?;
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(ref_name) = head.strip_prefix("ref: ") {
        let ref_name = ref_name.trim().to_string();
        let sha = resolve_ref(&git_dir, &ref_name)?;
        Some(crate::json::JsonGitInfo {
            sha,
            r#ref: Some(ref_name),
        })
    } else if is_sha(head) {
        // Detached HEAD holds the commit directly.
        Some(crate::json::JsonGitInfo {
            sha: head.to_string(),
            r#ref: None,
        })
    } else {
        None
    }
}

/// `.git` is a plain directory for regular checkouts; worktrees and
/// submodules use a file with a `gitdir: <path>` line pointing at the real
/// directory.
fn resolve_git_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let contents = std::fs::read_to_string(&dot_git).ok()?;
    let gitdir = contents.strip_prefix("gitdir:")?.trim();
    let path = Path::new(gitdir);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

/// Loose ref first (in the git dir, then the common dir shared between
/// worktrees), falling back to `packed-refs`.
fn resolve_ref(git_dir: &Path, ref_name: &str) -> Option<String> {
    let common_dir = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(rel) => {
            let rel = rel.trim();
            let path = Path::new(rel);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                git_dir.join(rel)
            }
        }
        Err(_) => git_dir.to_path_buf(),
    };

    for dir in [git_dir, common_dir.as_path()] {
        if let Ok(contents) = std::fs::read_to_string(dir.join(ref_name)) {
            let sha = contents.trim();
            if is_sha(sha) {
                return Some(sha.to_string());
            }
        }
    }

    let packed = std::fs::read_to_string(common_dir.join("packed-refs")).ok()?;
    for line in packed.lines() {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        if let Some((sha, name)) = line.split_once(' ') {
            if name.trim() == ref_name && is_sha(sha.trim()) {
                return Some(sha.trim().to_string());
            }
        }
    }
    None
}

fn is_sha(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use crate::lib_on::git_info::read_git_info_at;
    use std::path::PathBuf;

    fn temp_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hotpath-meta-git-info-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const SHA: &str = "4c36978500000000000000000000000000000000";

    #[test]
    fn reads_loose_ref() {
        let root = temp_repo("loose");
        let git = root.join(".git");
        std::fs::create_dir_all(git.join("refs/heads")).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(git.join("refs/heads/main"), format!("{SHA}\n")).unwrap();

        let info = read_git_info_at(&root).unwrap();
        assert_eq!(info.sha, SHA);
        assert_eq!(info.r#ref.as_deref(), Some("refs/heads/main"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_packed_ref() {
        let root = temp_repo("packed");
        let git = root.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/develop\n").unwrap();
        std::fs::write(
            git.join("packed-refs"),
            format!("# pack-refs with: peeled fully-peeled sorted\n{SHA} refs/heads/develop\n"),
        )
        .unwrap();

        let info = read_git_info_at(&root).unwrap();
        assert_eq!(info.sha, SHA);
        assert_eq!(info.r#ref.as_deref(), Some("refs/heads/develop"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_detached_head() {
        let root = temp_repo("detached");
        let git = root.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), format!("{SHA}\n")).unwrap();

        let info = read_git_info_at(&root).unwrap();
        assert_eq!(info.sha, SHA);
        assert_eq!(info.r#ref, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_worktree_gitdir_file() {
        let root = temp_repo("worktree");
        let real = root.join("real-git");
        std::fs::create_dir_all(real.join("worktrees/wt")).unwrap();
        std::fs::create_dir_all(real.join("refs/heads")).unwrap();
        let checkout = root.join("checkout");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", real.join("worktrees/wt").display()),
        )
        .unwrap();
        std::fs::write(real.join("worktrees/wt/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            real.join("worktrees/wt/commondir"),
            format!("{}\n", real.display()),
        )
        .unwrap();
        std::fs::write(real.join("refs/heads/main"), format!("{SHA}\n")).unwrap();

        let info = read_git_info_at(&checkout).unwrap();
        assert_eq!(info.sha, SHA);
        assert_eq!(info.r#ref.as_deref(), Some("refs/heads/main"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
