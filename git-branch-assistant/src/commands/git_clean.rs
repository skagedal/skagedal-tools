use std::env;
use std::path::PathBuf;

use anyhow::Result;

use crate::cleaner::GitCleaner;
use crate::git::GitRepo;
use crate::repository::Repository;
use crate::task_result::TaskResult;
use crate::ui::{DialoguerPrompt, DryRunPrompt};

pub fn run(path: Option<PathBuf>, dry: bool) -> Result<()> {
    let repo_path = path.map(Ok).unwrap_or_else(env::current_dir)?;
    let repo = GitRepo::new(repo_path);
    let branches = repo.get_branches()?;

    let result = if dry {
        let cleaner = GitCleaner::new_with_dry_run(DryRunPrompt, true);
        cleaner.handle(&repo, branches)?
    } else {
        let cleaner = GitCleaner::new(DialoguerPrompt);
        cleaner.handle(&repo, branches)?
    };

    match result {
        TaskResult::Proceed => Ok(()),
        TaskResult::ShellActionRequired(path) => {
            if !dry {
                Repository::new().set_suggested_directory(&path)?;
                std::process::exit(10);
            }
            Ok(())
        }
    }
}
