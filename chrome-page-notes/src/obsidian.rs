//! macOS-specific helpers for interacting with the Obsidian app.
//!
//! Looking up and creating notes doesn't need any of this - see `notes.rs`,
//! which reads/writes the vault folder directly. This module is only for
//! the one action that genuinely needs the app: opening a note in it. That
//! goes through the `obsidian://` URL scheme rather than the `obsidian`
//! CLI, because the CLI requires the app to already be running, and its
//! `vault=` option turned out to be silently ignored - every command
//! operated on whichever vault window happened to be focused, regardless
//! of what `vault=` was passed.

use std::process::Command;

use anyhow::{Context, Result, bail};

/// Opens a URL via macOS's `open`. Used for `obsidian://` URIs, which
/// Obsidian's own URL scheme handler resolves by vault name correctly, and
/// which auto-launches the app if it's not already running.
pub fn open_url(url: &str) -> Result<()> {
    let output = Command::new("open")
        .arg(url)
        .output()
        .context("failed to run `open`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`open` failed: {}", stderr.trim());
    }

    Ok(())
}

/// Brings the Obsidian app to the foreground (or launches it). Best
/// effort: failures here shouldn't fail the surrounding request.
pub fn activate_app() {
    let _ = Command::new("open").args(["-a", "Obsidian"]).output();
}
