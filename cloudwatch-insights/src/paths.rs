//! Per-tool state directory and config-file path resolution.

use std::path::{Path, PathBuf};

use crate::git;

const TOOL: &str = "cloudwatch-insights";

/// Absolute path to the config file. Resolution order:
///   1. `$CLOUDWATCH_INSIGHTS_CONFIG` (explicit override)
///   2. `$SKAGEDAL_TOOLS_HOME/cloudwatch-insights/settings.toml`
///   3. `~/.skagedal-tools/cloudwatch-insights/settings.toml`
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLOUDWATCH_INSIGHTS_CONFIG") {
        return PathBuf::from(p);
    }
    tool_base_dir().join("settings.toml")
}

/// Per-tool base directory: `<base>/cloudwatch-insights`, where `<base>`
/// is `$SKAGEDAL_TOOLS_HOME` or `~/.skagedal-tools`.
pub fn tool_base_dir() -> PathBuf {
    let base = match std::env::var("SKAGEDAL_TOOLS_HOME") {
        Ok(p) => PathBuf::from(p),
        Err(_) => dirs::home_dir()
            .expect("could not resolve home directory")
            .join(".skagedal-tools"),
    };
    base.join(TOOL)
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
    tool_base_dir()
        .join("queries")
        .join(current_slot(cwd))
        .join("current.insights")
}

pub fn results_dir(cwd: &Path) -> PathBuf {
    tool_base_dir()
        .join("queries")
        .join(current_slot(cwd))
        .join("results")
}

pub fn latest_run_path() -> PathBuf {
    tool_base_dir().join("latest-run.jsonl")
}
