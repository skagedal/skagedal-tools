use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::fs_utils::is_globally_ignored;
use crate::git::GitRepo;

#[derive(Debug, Clone)]
pub struct RepoActivityEntry {
    pub path: PathBuf,
    pub commit_timestamp: i64,
    pub commit_date: String,
}

/// Every git repository directly under `roots`, oldest latest-commit first.
/// Subdirectories that are not git repositories, or that have no commits yet,
/// are skipped rather than failing the whole scan.
pub fn collect_activity(roots: &[PathBuf]) -> Result<Vec<RepoActivityEntry>> {
    let mut entries = collect_entries(roots)?;
    entries.sort_by(|a, b| {
        a.commit_timestamp
            .cmp(&b.commit_timestamp)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(entries)
}

fn collect_entries(roots: &[PathBuf]) -> Result<Vec<RepoActivityEntry>> {
    let mut dir_paths = Vec::new();
    for root in roots {
        let read_dir =
            fs::read_dir(root).with_context(|| format!("failed to list {}", root.display()))?;
        for entry in read_dir {
            let path = entry
                .with_context(|| format!("failed to list {}", root.display()))?
                .path();
            if path.is_dir() && !is_globally_ignored(&path) {
                dir_paths.push(path);
            }
        }
    }

    Ok(dir_paths
        .par_iter()
        .filter_map(|path| entry_for(path))
        .collect())
}

fn entry_for(path: &Path) -> Option<RepoActivityEntry> {
    let info = GitRepo::new(path.to_path_buf()).latest_commit_info().ok()?;
    Some(RepoActivityEntry {
        path: path.to_path_buf(),
        commit_timestamp: info.commit_timestamp,
        commit_date: info.commit_date,
    })
}

pub fn format_entry_lines(entries: &[RepoActivityEntry]) -> Vec<String> {
    let date_width = entries
        .iter()
        .map(|entry| entry.commit_date.chars().count())
        .max()
        .unwrap_or(0);
    entries
        .iter()
        .map(|entry| {
            format!(
                "{date:<date_width$}  {path}",
                date = entry.commit_date,
                path = entry.path.display(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run(cwd: &Path, args: &[&str], date: Option<&str>) -> Result<()> {
        let mut command = Command::new(args[0]);
        command.current_dir(cwd).args(&args[1..]);
        if let Some(date) = date {
            command.env("GIT_AUTHOR_DATE", date);
            command.env("GIT_COMMITTER_DATE", date);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn init_repo_with_commit(repo: &Path, date: &str) -> Result<()> {
        fs::create_dir_all(repo)?;
        run(repo, &["git", "init", "-q", "-b", "main"], None)?;
        run(
            repo,
            &["git", "config", "user.email", "test@example.com"],
            None,
        )?;
        run(repo, &["git", "config", "user.name", "Test"], None)?;
        fs::write(repo.join("README.md"), "hello\n")?;
        run(repo, &["git", "add", "."], None)?;
        run(repo, &["git", "commit", "-q", "-m", "initial"], Some(date))?;
        Ok(())
    }

    #[test]
    fn latest_commit_info_returns_committer_date() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let repo = tmp.path().join("repo");
        init_repo_with_commit(&repo, "2024-06-15T12:34:56+00:00")?;

        let info = GitRepo::new(repo).latest_commit_info()?;

        assert_eq!(info.commit_timestamp, 1718454896);
        assert!(info.commit_date.starts_with("2024-06-15T"));
        Ok(())
    }

    #[test]
    fn entry_for_skips_directory_that_is_not_a_repo() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        assert!(entry_for(tmp.path()).is_none());
        Ok(())
    }

    #[test]
    fn repos_are_sorted_oldest_first_across_roots() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let first_root = tmp.path().join("a");
        let second_root = tmp.path().join("b");
        init_repo_with_commit(&first_root.join("newest"), "2024-09-20T00:00:00+00:00")?;
        init_repo_with_commit(&second_root.join("oldest"), "2023-08-12T00:00:00+00:00")?;
        init_repo_with_commit(&second_root.join("middle"), "2024-01-04T00:00:00+00:00")?;

        let entries = collect_activity(&[first_root, second_root])?;

        let names: Vec<String> = entries
            .iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, ["oldest", "middle", "newest"]);
        Ok(())
    }

    #[test]
    fn non_repo_subdirectories_and_files_are_skipped() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().join("root");
        init_repo_with_commit(&root.join("repo"), "2024-06-15T12:34:56+00:00")?;
        fs::create_dir_all(root.join("plain-directory"))?;
        fs::write(root.join("not-a-dir"), "x")?;

        let entries = collect_activity(&[root])?;

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path.file_name().unwrap(), "repo");
        Ok(())
    }

    #[test]
    fn entry_lines_pad_the_date_column() {
        let entries = vec![
            RepoActivityEntry {
                path: PathBuf::from("/tmp/one"),
                commit_timestamp: 1,
                commit_date: "2024-01-04T00:00:00+00:00".to_string(),
            },
            RepoActivityEntry {
                path: PathBuf::from("/tmp/two"),
                commit_timestamp: 2,
                commit_date: "2024-09-20T00:00:00+0100".to_string(),
            },
        ];

        let lines = format_entry_lines(&entries);

        assert_eq!(lines[0], "2024-01-04T00:00:00+00:00  /tmp/one");
        assert_eq!(lines[1], "2024-09-20T00:00:00+0100   /tmp/two");
    }
}
