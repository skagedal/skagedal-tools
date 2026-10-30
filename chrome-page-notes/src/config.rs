//! Vault configuration, read from this repo's per-tool config directory
//! (see `AGENTS.md`).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    /// Absolute path to the Obsidian vault's folder on disk, or one starting
    /// with `~/`. Notes are
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

pub fn config_path() -> PathBuf {
    skagedal_dirs::config_dir(crate::TOOL).join("config.toml")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "could not read config file at {} (expected `vault_path`)",
            path.display()
        )
    })?;
    let mut config: Config = toml::from_str(&text)
        .with_context(|| format!("could not parse config file at {}", path.display()))?;
    config.vault_path = expand_home(config.vault_path)?;

    if !config.vault_path.is_absolute() {
        bail!(
            "vault_path \"{}\" (from {}) must be an absolute path or start with ~/",
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

fn expand_home(path: PathBuf) -> Result<PathBuf> {
    match path.strip_prefix("~") {
        Ok(rest) => {
            let home = dirs::home_dir().context("could not resolve home directory")?;
            Ok(home.join(rest))
        }
        Err(_) => Ok(path),
    }
}
