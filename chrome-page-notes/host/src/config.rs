//! Vault/folder configuration, read from a dotfile following this repo's
//! per-tool config convention (see `AGENTS.md`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli;

#[derive(Deserialize)]
pub struct Config {
    pub vault: String,
    #[serde(default = "default_folder")]
    pub folder: String,
    /// Absolute path to the `obsidian` CLI binary. Must be absolute
    /// because this host is spawned by Chrome with a minimal PATH that
    /// won't find it otherwise (see `cli.rs`).
    pub obsidian_binary: String,
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
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "could not read config file at {} (expected `vault`, `obsidian_binary`, entries)",
            path.display()
        )
    })?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("could not parse config file at {}", path.display()))?;

    let binary_path = Path::new(&config.obsidian_binary);
    if !binary_path.is_absolute() {
        bail!(
            "obsidian_binary \"{}\" (from {}) must be an absolute path",
            config.obsidian_binary,
            path.display()
        );
    }
    if !binary_path.is_file() {
        bail!(
            "obsidian_binary \"{}\" (from {}) does not exist",
            config.obsidian_binary,
            path.display()
        );
    }

    let known_vaults = list_vaults(&config.obsidian_binary)?;
    if !known_vaults.iter().any(|v| v == &config.vault) {
        bail!(
            "vault \"{}\" (from {}) is not among Obsidian's known vaults: {}",
            config.vault,
            path.display(),
            known_vaults.join(", ")
        );
    }

    Ok(config)
}

fn list_vaults(obsidian_binary: &str) -> Result<Vec<String>> {
    let output = cli::run(obsidian_binary, &["vaults".to_string()])?;
    Ok(output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}
