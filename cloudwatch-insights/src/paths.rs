//! Per-tool config and data paths.

use std::path::{Path, PathBuf};

use crate::git;

const TOOL: &str = "cloudwatch-insights";

/// Absolute path to the config file. Resolution order:
///   1. `$CLOUDWATCH_INSIGHTS_CONFIG` (explicit override)
///   2. `$XDG_CONFIG_HOME/skagedal-tools/cloudwatch-insights/settings.toml`
///   3. `~/.config/skagedal-tools/cloudwatch-insights/settings.toml`
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLOUDWATCH_INSIGHTS_CONFIG") {
        return PathBuf::from(p);
    }
    skagedal_dirs::config_dir(TOOL).join("settings.toml")
}

/// Where saved queries and downloaded results live:
/// `~/.local/share/skagedal-tools/cloudwatch-insights`.
pub fn data_dir() -> PathBuf {
    skagedal_dirs::data_dir(TOOL)
}

/// Slot name used for the queries subtree — the repo's basename, or
/// `_default` when run outside a git repository.
pub fn current_slot(cwd: &Path) -> String {
    match git::find_git_root(cwd) {
        Some(root) => root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("_default")
            .to_string(),
        None => "_default".into(),
    }
}

pub fn current_insights_path(cwd: &Path) -> PathBuf {
    data_dir()
        .join("queries")
        .join(current_slot(cwd))
        .join("current.insights")
}

pub fn results_dir(cwd: &Path) -> PathBuf {
    data_dir()
        .join("queries")
        .join(current_slot(cwd))
        .join("results")
}

pub fn latest_run_path() -> PathBuf {
    data_dir().join("latest-run.jsonl")
}
