use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;

use crate::bulk_picker;
use crate::cleaner::{ActionResult, BranchAction, GitCleaner, actions_for_branch, state_label};
use crate::fs_utils::is_globally_ignored;
use crate::git::{Branch, GitRepo, PrOptions, UpstreamStatus};
use crate::pr_options_picker::{self, PendingPr, PrOptionsOutcome};
use crate::task_result::TaskResult;
use crate::ui::DialoguerPrompt;

pub struct GitReposBulkService {
    skip_dirty_repos: bool,
}

#[derive(Debug)]
struct BulkBranch {
    repo_name: String,
    repo_path: PathBuf,
    branch: Branch,
}

impl BulkBranch {
    fn display(&self) -> String {
        format!("{}/{}", self.repo_name, self.branch.refname)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GroupKey {
    NoUpstream,
    UpstreamIsAheadOfLocal,
    LocalIsAheadOfUpstream,
    MergeNeeded,
    UpstreamIsGone,
}

impl GroupKey {
    fn order_index(self) -> usize {
        match self {
            GroupKey::NoUpstream => 0,
            GroupKey::UpstreamIsAheadOfLocal => 1,
            GroupKey::LocalIsAheadOfUpstream => 2,
            GroupKey::MergeNeeded => 3,
            GroupKey::UpstreamIsGone => 4,
        }
    }

    fn from_branch(branch: &Branch) -> Option<Self> {
        match &branch.upstream {
            None => Some(GroupKey::NoUpstream),
            Some(upstream) => match upstream.status {
                UpstreamStatus::Identical => None,
                UpstreamStatus::UpstreamIsAheadOfLocal => Some(GroupKey::UpstreamIsAheadOfLocal),
                UpstreamStatus::LocalIsAheadOfUpstream => Some(GroupKey::LocalIsAheadOfUpstream),
                UpstreamStatus::MergeNeeded => Some(GroupKey::MergeNeeded),
                UpstreamStatus::UpstreamIsGone => Some(GroupKey::UpstreamIsGone),
            },
        }
    }
}

impl GitReposBulkService {
    pub fn new(skip_dirty_repos: bool) -> Self {
        Self { skip_dirty_repos }
    }

    pub fn handle_all(&self, path: &Path) -> Result<TaskResult> {
        let branches = self.collect_branches(path)?;
        let mut groups = group_by_state(branches);
        groups.sort_by_key(|(key, _)| key.order_index());

        for (_key, mut group) in groups {
            eprintln!();
            loop {
                if group.is_empty() {
                    break;
                }
                let actions = self.bulk_actions_for_group(&group);
                if actions.is_empty() {
                    break;
                }
                if !bulk_picker::stderr_is_terminal() {
                    eprintln!(
                        "Bulk actions require an interactive terminal; skipping {} branches.",
                        group.len()
                    );
                    break;
                }

                let title = state_label(&group[0].branch);
                let entries: Vec<String> = group.iter().map(|b| b.display()).collect();
                let action_specs: Vec<bulk_picker::ActionSpec> = actions
                    .iter()
                    .map(|a| bulk_picker::ActionSpec {
                        label: a.description().to_string(),
                        bulk_safe: a.is_bulk_safe(),
                    })
                    .collect();

                let outcome = bulk_picker::run(title, &entries, &action_specs)?;
                let (target_indices, action_index) = match outcome {
                    bulk_picker::BulkOutcome::Cancelled => break,
                    bulk_picker::BulkOutcome::Confirmed {
                        target_indices,
                        action_index,
                    } => (target_indices, action_index),
                };

                if target_indices.is_empty() {
                    eprintln!("No branches selected; skipping group.");
                    break;
                }

                let action = actions[action_index];

                let pr_options = if matches!(action, BranchAction::CreatePr) {
                    match self.confirm_pr_options(&group, &target_indices)? {
                        Some(options) => Some(options),
                        None => continue,
                    }
                } else {
                    None
                };

                let cleaner = GitCleaner::new(DialoguerPrompt);
                if action.is_bulk_safe() {
                    eprintln!(
                        "Applying \"{}\" to {} branch{}...",
                        action.description(),
                        target_indices.len(),
                        if target_indices.len() == 1 { "" } else { "es" }
                    );
                } else {
                    let bulk_branch = &group[target_indices[0]];
                    eprintln!("{} on {}...", action.description(), bulk_branch.display());
                }

                let mut handled: std::collections::HashSet<usize> =
                    std::collections::HashSet::new();
                for &idx in &target_indices {
                    let bulk_branch = &group[idx];
                    let repo = GitRepo::new(bulk_branch.repo_path.clone());
                    eprintln!("  {}: {}", bulk_branch.display(), action.description());
                    let result = if let Some(options) = pr_options {
                        run_create_pr(&repo, &bulk_branch.branch, options)?
                    } else {
                        cleaner.perform_action(&repo, &bulk_branch.branch, action)?
                    };
                    match result {
                        ActionResult::Handled => {
                            handled.insert(idx);
                        }
                        ActionResult::NotHandled => {}
                        ActionResult::ExitToShell(path) => {
                            return Ok(TaskResult::ShellActionRequired(path));
                        }
                    }
                }

                group = group
                    .into_iter()
                    .enumerate()
                    .filter(|(idx, _)| !handled.contains(idx))
                    .map(|(_, b)| b)
                    .collect();
            }
        }

        Ok(TaskResult::Proceed)
    }

    fn collect_branches(&self, path: &Path) -> Result<Vec<BulkBranch>> {
        let dir_paths: Vec<PathBuf> = fs::read_dir(path)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.is_dir() && !is_globally_ignored(p))
            .collect();

        let collected: Vec<Vec<BulkBranch>> = dir_paths
            .par_iter()
            .map(|entry_path| self.collect_repo_branches(entry_path).unwrap_or_default())
            .collect();

        let mut result: Vec<BulkBranch> = collected.into_iter().flatten().collect();
        result.sort_by(|a, b| {
            a.repo_name
                .cmp(&b.repo_name)
                .then_with(|| a.branch.refname.cmp(&b.branch.refname))
        });
        Ok(result)
    }

    fn collect_repo_branches(&self, dir: &Path) -> Result<Vec<BulkBranch>> {
        let repo = GitRepo::new(dir.to_path_buf());
        if self.skip_dirty_repos && repo.is_dirty().unwrap_or(false) {
            return Ok(Vec::new());
        }
        let repo_name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());

        let branches = repo.get_branches()?;
        let result = branches
            .into_iter()
            .filter(|b| b.needs_action())
            .map(|branch| BulkBranch {
                repo_name: repo_name.clone(),
                repo_path: dir.to_path_buf(),
                branch,
            })
            .collect();
        Ok(result)
    }

    fn confirm_pr_options(
        &self,
        group: &[BulkBranch],
        target_indices: &[usize],
    ) -> Result<Option<PrOptions>> {
        let pending: Vec<PendingPr> = target_indices
            .par_iter()
            .map(|&idx| {
                let bulk_branch = &group[idx];
                let repo = GitRepo::new(bulk_branch.repo_path.clone());
                let base = repo.default_branch().unwrap_or_else(|err| {
                    eprintln!(
                        "Could not determine default branch for {}: {err}. Falling back to 'main'.",
                        bulk_branch.repo_name
                    );
                    "main".to_string()
                });
                PendingPr {
                    repo_display: bulk_branch.repo_name.clone(),
                    head: bulk_branch.branch.refname.clone(),
                    base,
                }
            })
            .collect();

        if !bulk_picker::stderr_is_terminal() {
            eprintln!("Bulk PR creation requires an interactive terminal; skipping.");
            return Ok(None);
        }

        match pr_options_picker::run(&pending)? {
            PrOptionsOutcome::Confirmed(options) => Ok(Some(options)),
            PrOptionsOutcome::Cancelled => Ok(None),
        }
    }

    fn bulk_actions_for_group(&self, group: &[BulkBranch]) -> Vec<BranchAction> {
        let Some(first) = group.first() else {
            return Vec::new();
        };
        let repo = GitRepo::new(first.repo_path.clone());
        actions_for_branch(&first.branch, &repo)
            .into_iter()
            .filter(|action| !matches!(action, BranchAction::DeleteWorktreeAndBranch))
            .collect()
    }
}

fn run_create_pr(repo: &GitRepo, branch: &Branch, options: PrOptions) -> Result<ActionResult> {
    repo.push_creating_origin(&branch.refname)?;
    let base = repo.default_branch()?;
    repo.create_pull_request(&branch.refname, &base, options)?;
    Ok(ActionResult::Handled)
}

fn group_by_state(branches: Vec<BulkBranch>) -> Vec<(GroupKey, Vec<BulkBranch>)> {
    let mut map: std::collections::BTreeMap<usize, (GroupKey, Vec<BulkBranch>)> =
        std::collections::BTreeMap::new();
    for bb in branches {
        let Some(key) = GroupKey::from_branch(&bb.branch) else {
            continue;
        };
        map.entry(key.order_index())
            .or_insert_with(|| (key, Vec::new()))
            .1
            .push(bb);
    }
    map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Upstream, UpstreamStatus};

    fn bb(repo: &str, refname: &str, upstream: Option<UpstreamStatus>) -> BulkBranch {
        BulkBranch {
            repo_name: repo.to_string(),
            repo_path: PathBuf::from("/tmp").join(repo),
            branch: Branch {
                refname: refname.to_string(),
                upstream: upstream.map(|status| Upstream {
                    name: format!("origin/{refname}"),
                    status,
                }),
                worktree_path: None,
            },
        }
    }

    #[test]
    fn group_by_state_groups_and_sorts() {
        let branches = vec![
            bb("a", "feature-1", Some(UpstreamStatus::UpstreamIsGone)),
            bb("b", "main", Some(UpstreamStatus::Identical)),
            bb("c", "new", None),
            bb("a", "feature-2", Some(UpstreamStatus::UpstreamIsGone)),
        ];
        let groups = group_by_state(branches);
        let keys: Vec<GroupKey> = groups.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![GroupKey::NoUpstream, GroupKey::UpstreamIsGone]);
        let gone_group = groups
            .iter()
            .find(|(k, _)| *k == GroupKey::UpstreamIsGone)
            .unwrap();
        assert_eq!(gone_group.1.len(), 2);
    }

    #[test]
    fn bulk_actions_excludes_delete_worktree_and_branch() {
        let service = GitReposBulkService::new(false);
        let group = vec![bb("a", "x", Some(UpstreamStatus::UpstreamIsGone))];
        let actions = service.bulk_actions_for_group(&group);
        assert!(!actions.is_empty());
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, BranchAction::DeleteWorktreeAndBranch))
        );
    }

    #[test]
    fn bulk_actions_for_no_upstream_includes_create_pr() {
        let service = GitReposBulkService::new(false);
        let group = vec![bb("a", "x", None)];
        let actions = service.bulk_actions_for_group(&group);
        assert!(actions.iter().any(|a| matches!(a, BranchAction::CreatePr)));
    }
}
