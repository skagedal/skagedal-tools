//! Thin wrapper for invoking the `obsidian` CLI.
//!
//! Chrome launches this host with a minimal PATH that doesn't include
//! Homebrew's bin directories, so a bare `Command::new("obsidian")` fails
//! even though it works fine from a terminal. Rather than guessing at
//! install locations, the binary's absolute path comes from config (see
//! `config::Config::obsidian_binary`).

use std::process::Command;

use anyhow::{Context, Result, bail};

/// Runs the `obsidian` CLI and returns its stdout.
///
/// A nonzero exit is treated as a hard error (distinct from the CLI's habit
/// of exiting 0 and printing `Error: ...` to stdout for things like "file
/// not found") — e.g. when the Obsidian app itself isn't running, the CLI
/// exits 1 and writes "The CLI is unable to find Obsidian..." to stderr.
pub fn run(binary: &str, args: &[String]) -> Result<String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .with_context(|| format!("failed to run the obsidian CLI at \"{binary}\""))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("obsidian CLI failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Brings the Obsidian app to the foreground. `obsidian open`/`obsidian
/// create` navigate to the note within the app but don't activate its
/// window, so this uses macOS's `open -a` to do that separately. Best
/// effort: failures here shouldn't fail the surrounding request, since the
/// note action itself already succeeded.
pub fn activate_app() {
    let _ = Command::new("open").args(["-a", "Obsidian"]).output();
}
