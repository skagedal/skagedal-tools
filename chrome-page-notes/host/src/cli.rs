//! Thin wrapper for invoking the `obsidian` CLI.
//!
//! Chrome launches this host with a minimal PATH that doesn't include
//! Homebrew's bin directories, so a bare `Command::new("obsidian")` fails
//! even though it works fine from a terminal. Rather than guessing at
//! install locations, the binary's absolute path comes from config (see
//! `config::Config::obsidian_binary`).

use std::process::Command;

use anyhow::{Context, Result};

pub fn run(binary: &str, args: &[String]) -> Result<String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .with_context(|| format!("failed to run the obsidian CLI at \"{binary}\""))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
