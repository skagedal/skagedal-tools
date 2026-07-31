//! Vault configuration, read from a dotfile following this repo's per-tool
//! config convention (see `AGENTS.md`).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    /// Absolute path to the Obsidian vault's folder on disk. Notes are
    /// read and written directly here (see `notes.rs`) rather than through
    /// the `obsidian` CLI.
    pub vault_path: PathBuf,
    #[serde(default = "default_folder")]
    pub folder: String,
    /// Whether to append every incoming message to host.log. Off by
    /// default since every page visit would otherwise write to a log file
    /// that's never trimmed or rotated.
    #[serde(default)]
    pub debug: bool,
}

fn default_folder() -> String {
    "webnotes".to_string()
}

fn tool_base_dir() -> PathBuf {
    let base = match std::env::var("SKAGEDAL_TOOLS_HOME") {
        Ok(p) => PathBuf::from(p),
        Err(_) => dirs::home_dir()
            .expect("could not resolve home directory")
            .join(".skagedal-tools"),
    };
    base.join("chrome-page-notes")
}

pub fn config_path() -> PathBuf {
    tool_base_dir().join("config.toml")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read config file at {} (expected `vault_path`)", path.display()))?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("could not parse config file at {}", path.display()))?;

    if !config.vault_path.is_absolute() {
        bail!(
            "vault_path \"{}\" (from {}) must be an absolute path",
            config.vault_path.display(),
            path.display()
        );
    }
    if !config.vault_path.is_dir() {
        bail!(
            "vault_path \"{}\" (from {}) does not exist or is not a directory",
            config.vault_path.display(),
            path.display()
        );
    }

    Ok(config)
}
