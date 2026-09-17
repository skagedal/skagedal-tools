//! Where the tool keeps its configuration.

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
