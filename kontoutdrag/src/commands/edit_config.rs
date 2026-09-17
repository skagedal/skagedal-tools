use anyhow::{Context, Result};

use crate::config;
use crate::paths;

pub fn run() -> Result<()> {
    let path = paths::config_path();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        std::fs::write(&path, config::TEMPLATE)
            .with_context(|| format!("could not write {}", path.display()))?;
        eprintln!("created {}", path.display());
    }

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("could not run {editor}"))?;
    if !status.success() {
        anyhow::bail!("{editor} exited with {status}");
    }

    // Parse what was written, so a typo is reported now rather than on the
    // next run.
    config::load(&path)?.load_tables()?;
    Ok(())
}
