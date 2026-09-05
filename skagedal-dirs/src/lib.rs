//! XDG-style per-tool directories, on every platform.
//!
//! Every tool in this repo keeps its user-scoped files under a
//! `skagedal-tools/<tool>` subdirectory of one of the three XDG base
//! directories:
//!
//! ```text
//! ~/.config/skagedal-tools/<tool>/        config
//! ~/.local/share/skagedal-tools/<tool>/   data and state
//! ~/.cache/skagedal-tools/<tool>/         throwaway caches
//! ```
//!
//! The XDG layout is used on macOS too, rather than the
//! `~/Library/Application Support/` convention: keeping every tool's files
//! in the same place on every machine matters more here than following
//! Apple's conventions, and `~/Library/Application Support/tech.skagedal.…`
//! is a mouthful to type.
//!
//! `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME` and `$XDG_CACHE_HOME` override the
//! respective roots when set to an absolute path, per the XDG base
//! directory specification — that's also how tests point a tool at a
//! temporary directory.

use std::path::{Path, PathBuf};

/// The directory every tool's files are grouped under, inside each XDG root.
const NAMESPACE: &str = "skagedal-tools";

/// `~/.config/skagedal-tools/<tool>`, or `$XDG_CONFIG_HOME/skagedal-tools/<tool>`.
pub fn config_dir(tool: &str) -> PathBuf {
    tool_dir("XDG_CONFIG_HOME", ".config", tool)
}

/// `~/.local/share/skagedal-tools/<tool>`, or `$XDG_DATA_HOME/skagedal-tools/<tool>`.
pub fn data_dir(tool: &str) -> PathBuf {
    tool_dir("XDG_DATA_HOME", ".local/share", tool)
}

/// `~/.cache/skagedal-tools/<tool>`, or `$XDG_CACHE_HOME/skagedal-tools/<tool>`.
pub fn cache_dir(tool: &str) -> PathBuf {
    tool_dir("XDG_CACHE_HOME", ".cache", tool)
}

fn tool_dir(var: &str, fallback: &str, tool: &str) -> PathBuf {
    let home = dirs::home_dir().expect("could not resolve home directory");
    resolve_base(std::env::var(var).ok().as_deref(), &home, fallback)
        .join(NAMESPACE)
        .join(tool)
}

/// The XDG root: the environment variable when it holds an absolute path,
/// else `$HOME/<fallback>`. The specification says a relative (or empty)
/// value must be ignored as invalid.
fn resolve_base(var: Option<&str>, home: &Path, fallback: &str) -> PathBuf {
    match var {
        Some(value) if Path::new(value).is_absolute() => PathBuf::from(value),
        _ => home.join(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/simon")
    }

    #[test]
    fn falls_back_to_home_when_unset() {
        assert_eq!(
            resolve_base(None, &home(), ".config"),
            PathBuf::from("/home/simon/.config")
        );
        assert_eq!(
            resolve_base(None, &home(), ".local/share"),
            PathBuf::from("/home/simon/.local/share")
        );
    }

    #[test]
    fn absolute_env_var_wins() {
        assert_eq!(
            resolve_base(Some("/tmp/xdg"), &home(), ".config"),
            PathBuf::from("/tmp/xdg")
        );
    }

    #[test]
    fn ignores_empty_and_relative_env_var() {
        assert_eq!(
            resolve_base(Some(""), &home(), ".cache"),
            PathBuf::from("/home/simon/.cache")
        );
        assert_eq!(
            resolve_base(Some("relative/path"), &home(), ".cache"),
            PathBuf::from("/home/simon/.cache")
        );
    }

    #[test]
    fn tool_dirs_are_namespaced() {
        let dir = config_dir("tracker");
        assert!(dir.ends_with("skagedal-tools/tracker"));
        assert!(cache_dir("gba").ends_with("skagedal-tools/gba"));
        assert!(data_dir("gba").ends_with("skagedal-tools/gba"));
    }
}
