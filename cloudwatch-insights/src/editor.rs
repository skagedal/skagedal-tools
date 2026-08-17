//! Spawn `$VISUAL` / `$EDITOR` on a path.

use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow};

use crate::commands::ExitError;

pub fn open_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()))
        .ok_or_else(|| {
            anyhow!(ExitError {
                code: 2,
                message: "no editor set — define $EDITOR (or $VISUAL) and retry".into(),
            })
        })?;

    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (cmd, args) = parts
        .split_first()
        .ok_or_else(|| anyhow!("editor command is empty"))?;

    let status = Command::new(cmd)
        .args(args)
        .arg(path)
        .status()
        .map_err(|e| anyhow!("failed to launch editor ({editor}): {e}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(1) as u8;
        return Err(anyhow!(ExitError {
            code,
            message: format!("editor exited with status {}", status.code().unwrap_or(1)),
        }));
    }
    Ok(())
}
