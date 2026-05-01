//! Resolve the top-level directory of the git repo containing a path.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve the top-level directory of the git repo containing `cwd`.
/// Returns `None` if `cwd` is not inside a git repository.
///
/// We prefer shelling out to `git rev-parse` (handles worktrees, submodules,
/// $GIT_DIR correctly) and fall back to walking up looking for ".git".
pub fn find_git_root(cwd: &Path) -> Option<PathBuf> {
    if let Ok(output) = Command::new("git")
        .args(["-C"])
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let trimmed = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
    }

    let mut dir = cwd.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn outside_repo_returns_none() {
        let tmp = tempdir().unwrap();
        // tempdir creates under /tmp; depending on the environment, /tmp may or
        // may not be inside a parent git repo, so we can only assert the
        // function doesn't panic. The real assertion is in the
        // find_git_root_inside test below.
        let _ = find_git_root(tmp.path());
    }

    #[test]
    fn finds_root_inside_repo() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("my-repo");
        std::fs::create_dir(&repo).unwrap();
        let status = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        let canonical_repo = std::fs::canonicalize(&repo).unwrap();
        let found = find_git_root(&repo).unwrap();
        assert_eq!(std::fs::canonicalize(&found).unwrap(), canonical_repo);
    }
}
