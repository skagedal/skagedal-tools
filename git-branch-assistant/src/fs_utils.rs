use std::env;
use std::path::{Path, PathBuf};

pub fn home_dir() -> Option<PathBuf> {
    env::var("HOME").map(PathBuf::from).ok()
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix('~')
        && let Some(home) = home_dir()
    {
        if stripped.is_empty() {
            return home;
        }
        return home.join(stripped.trim_start_matches('/'));
    }
    PathBuf::from(path)
}

pub fn is_globally_ignored(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(".DS_Store"))
        .unwrap_or(false)
}
