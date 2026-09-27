//! Where the tool keeps its configuration, and the comments written in the view.

use std::path::PathBuf;

const TOOL: &str = "kontoutdrag";

/// Absolute path to `settings.toml`. Resolution order:
///   1. `$KONTOUTDRAG_CONFIG` (explicit override)
///   2. `$XDG_CONFIG_HOME/skagedal-tools/kontoutdrag/settings.toml`
///   3. `~/.config/skagedal-tools/kontoutdrag/settings.toml`
pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("KONTOUTDRAG_CONFIG") {
        return PathBuf::from(path);
    }
    skagedal_dirs::config_dir(TOOL).join("settings.toml")
}

/// Where comments written in `kontoutdrag view` are kept:
/// `$XDG_DATA_HOME/skagedal-tools/kontoutdrag/comments.json`, by default
/// `~/.local/share/skagedal-tools/kontoutdrag/comments.json`. Make it a
/// symlink to keep the comments somewhere else, a git repository say; they
/// are written through it.
pub fn comments_path() -> PathBuf {
    skagedal_dirs::data_dir(TOOL).join("comments.json")
}
