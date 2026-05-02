use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::BranchCache;
use crate::fs_utils::is_globally_ignored;
use crate::git::{Branch, GitRepo, UpstreamStatus};
use crate::picker::{self, PickerOutcome};
use crate::task_result::TaskResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchListEntry {
    pub repo_name: String,
    pub repo_path: PathBuf,
    pub refname: String,
    pub status: BranchStatus,
    pub commit_timestamp: i64,
    pub commit_date: String,
    pub committer: String,
    pub worktree_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BranchStatus {
    Identical,
    UpstreamAhead,
    LocalAhead,
    Diverged,
    UpstreamGone,
    NoUpstream,
}

impl BranchStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Identical => "ok",
            Self::UpstreamAhead => "behind",
            Self::LocalAhead => "ahead",
            Self::Diverged => "diverged",
            Self::UpstreamGone => "gone",
            Self::NoUpstream => "no upstream",
        }
    }
}

pub struct GitReposListService {
    interactive: bool,
}

impl GitReposListService {
    pub fn new(interactive: bool) -> Self {
        Self { interactive }
    }

    pub fn list_all_branches(&self, path: &Path) -> Result<TaskResult> {
        if self.interactive && picker::stderr_is_terminal() {
            self.run_interactive(path)
        } else {
            self.run_non_interactive(path)
        }
    }

    fn run_non_interactive(&self, path: &Path) -> Result<TaskResult> {
        eprintln!("Collecting branches...");
        let entries = collect_and_sort(path)?;
        if let Some(cache) = BranchCache::from_env() {
            let _ = cache.write(path, &entries);
        }
        print_entries(&entries);
        Ok(TaskResult::Proceed)
    }

    fn run_interactive(&self, path: &Path) -> Result<TaskResult> {
        let cache = BranchCache::from_env();
        let cached = cache.as_ref().and_then(|c| c.read_fresh(path));

        let (initial, refresh_rx) = match cached {
            Some(cache_entries) => {
                let (tx, rx) = mpsc::channel();
                let scan_path = path.to_path_buf();
                thread::spawn(move || {
                    let entries = collect_and_sort(&scan_path).unwrap_or_default();
                    if let Some(cache) = BranchCache::from_env() {
                        let _ = cache.write(&scan_path, &entries);
                    }
                    let _ = tx.send(entries);
                });
                (cache_entries, Some(rx))
            }
            None => {
                eprintln!("Collecting branches...");
                let entries = collect_and_sort(path)?;
                if let Some(cache) = cache {
                    let _ = cache.write(path, &entries);
                }
                (entries, None)
            }
        };

        if initial.is_empty() && refresh_rx.is_none() {
            println!("No branches found.");
            return Ok(TaskResult::Proceed);
        }

        match picker::run(initial, refresh_rx)? {
            PickerOutcome::Picked(entry) => select_entry(&entry),
            PickerOutcome::Cancelled => Ok(TaskResult::Proceed),
        }
    }
}

fn collect_and_sort(path: &Path) -> Result<Vec<BranchListEntry>> {
    let mut entries = collect_branch_entries(path)?;
    entries.sort_by(|a, b| {
        a.commit_timestamp
            .cmp(&b.commit_timestamp)
            .then_with(|| a.repo_name.cmp(&b.repo_name))
            .then_with(|| a.refname.cmp(&b.refname))
    });
    Ok(entries)
}

fn collect_branch_entries(path: &Path) -> Result<Vec<BranchListEntry>> {
    let dir_paths: Vec<PathBuf> = fs::read_dir(path)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && !is_globally_ignored(p))
        .collect();

    let entries: Vec<BranchListEntry> = dir_paths
        .par_iter()
        .flat_map(|entry_path| collect_repo_entries(entry_path).unwrap_or_default())
        .collect();

    Ok(entries)
}

fn collect_repo_entries(entry_path: &Path) -> Result<Vec<BranchListEntry>> {
    let repo = GitRepo::new(entry_path.to_path_buf());
    let branches = repo.get_branches()?;
    let commit_infos = repo.branch_commit_infos()?;
    let repo_name = entry_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| entry_path.to_string_lossy().into_owned());

    let mut entries = Vec::new();
    for branch in branches {
        let Some(info) = commit_infos.get(&branch.refname) else {
            continue;
        };
        entries.push(BranchListEntry {
            repo_name: repo_name.clone(),
            repo_path: entry_path.to_path_buf(),
            refname: branch.refname.clone(),
            status: branch_status(&branch),
            commit_timestamp: info.commit_timestamp,
            commit_date: info.commit_date.clone(),
            committer: info.committer.clone(),
            worktree_path: branch.worktree_path.clone(),
        });
    }
    Ok(entries)
}

fn branch_status(branch: &Branch) -> BranchStatus {
    match branch.upstream.as_ref() {
        None => BranchStatus::NoUpstream,
        Some(upstream) => match upstream.status {
            UpstreamStatus::Identical => BranchStatus::Identical,
            UpstreamStatus::UpstreamIsAheadOfLocal => BranchStatus::UpstreamAhead,
            UpstreamStatus::LocalIsAheadOfUpstream => BranchStatus::LocalAhead,
            UpstreamStatus::MergeNeeded => BranchStatus::Diverged,
            UpstreamStatus::UpstreamIsGone => BranchStatus::UpstreamGone,
        },
    }
}

fn print_entries(entries: &[BranchListEntry]) {
    for line in format_entry_lines(entries) {
        println!("{line}");
    }
}

pub fn format_entry_lines(entries: &[BranchListEntry]) -> Vec<String> {
    if entries.is_empty() {
        return Vec::new();
    }
    let status_width = entries
        .iter()
        .map(|entry| entry.status.label().len())
        .max()
        .unwrap_or(0);
    let committer_width = entries
        .iter()
        .map(|entry| entry.committer.chars().count())
        .max()
        .unwrap_or(0);
    let location_width = entries
        .iter()
        .map(|entry| entry.repo_name.chars().count() + 1 + entry.refname.chars().count())
        .max()
        .unwrap_or(0);
    entries
        .iter()
        .map(|entry| {
            let location = format!("{}/{}", entry.repo_name, entry.refname);
            format!(
                "{date}  {status:<status_width$}  {committer:<committer_width$}  {location:<location_width$}",
                date = entry.commit_date,
                status = entry.status.label(),
                committer = entry.committer,
                location = location,
                status_width = status_width,
                committer_width = committer_width,
                location_width = location_width,
            )
        })
        .collect()
}

fn select_entry(entry: &BranchListEntry) -> Result<TaskResult> {
    if let Some(worktree_path) = &entry.worktree_path
        && !paths_equivalent(worktree_path, &entry.repo_path)
    {
        println!(
            "Branch '{}' is checked out in worktree {}",
            entry.refname,
            worktree_path.display()
        );
        return Ok(TaskResult::ShellActionRequired(worktree_path.clone()));
    }
    let repo = GitRepo::new(entry.repo_path.clone());
    repo.checkout_branch(&entry.refname)?;
    Ok(TaskResult::ShellActionRequired(entry.repo_path.clone()))
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a_can), Ok(b_can)) => a_can == b_can,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn entry(timestamp: i64, repo: &str, refname: &str) -> BranchListEntry {
        BranchListEntry {
            repo_name: repo.to_string(),
            repo_path: PathBuf::from("/tmp").join(repo),
            refname: refname.to_string(),
            status: BranchStatus::Identical,
            commit_timestamp: timestamp,
            commit_date: "2024-01-01".to_string(),
            committer: "alice".to_string(),
            worktree_path: None,
        }
    }

    #[test]
    fn entries_sort_oldest_first() {
        let mut entries = [
            entry(2000, "repo-a", "main"),
            entry(1000, "repo-b", "main"),
            entry(3000, "repo-a", "feature"),
        ];
        entries.sort_by(|a, b| {
            a.commit_timestamp
                .cmp(&b.commit_timestamp)
                .then_with(|| a.repo_name.cmp(&b.repo_name))
                .then_with(|| a.refname.cmp(&b.refname))
        });
        assert_eq!(entries[0].repo_name, "repo-b");
        assert_eq!(entries[1].repo_name, "repo-a");
        assert_eq!(entries[1].refname, "main");
        assert_eq!(entries[2].refname, "feature");
    }

    #[test]
    fn select_entry_redirects_to_worktree_path() -> Result<()> {
        let temp_repo = tempfile::tempdir()?;
        let temp_worktree = tempfile::tempdir()?;
        let mut entry = entry(1000, "repo", "feature");
        entry.repo_path = temp_repo.path().to_path_buf();
        entry.worktree_path = Some(temp_worktree.path().to_path_buf());

        let result = select_entry(&entry)?;
        match result {
            TaskResult::ShellActionRequired(path) => {
                assert_eq!(path, temp_worktree.path().to_path_buf());
            }
            TaskResult::Proceed => panic!("expected shell action"),
        }
        Ok(())
    }

    #[test]
    fn non_interactive_list_proceeds() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let service = GitReposListService::new(false);
        let result = service.list_all_branches(temp.path())?;
        assert!(matches!(result, TaskResult::Proceed));
        Ok(())
    }
}
