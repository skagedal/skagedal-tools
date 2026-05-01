use anyhow::Result;

use crate::config::ensure_config_file;
use crate::editor::open_editor;
use crate::paths;

pub async fn run() -> Result<()> {
    let path = paths::config_path();
    let cwd = std::env::current_dir()?;
    let result = ensure_config_file(&path, &cwd)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if result.file_created {
        eprintln!("Created {}", path.display());
    }
    if let Some(name) = result.added_section {
        eprintln!(
            "Added commented placeholder section [repo.{name}] for the current repository"
        );
    }
    open_editor(&path)?;
    Ok(())
}
