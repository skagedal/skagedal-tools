//! The `register` subcommand: drops a native messaging host manifest in
//! Chrome's `NativeMessagingHosts` directory, naming this binary as the
//! host for the extension, and says how to load the extension itself. See
//! <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging#native-messaging-host>.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};

use crate::{EXTENSION_ID, HOST_NAME, TOOL, config};

/// The extension's files, baked in so `register` has something to unpack
/// on a machine without this repo checked out.
static EXTENSION: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/extension");

/// Where `extension/` sat when this binary was compiled. `--dev` points at
/// it so edits take effect on a reload in Chrome rather than a reinstall;
/// stale if the repo has moved since, hence not the default.
pub const SOURCE_EXTENSION_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/extension");

fn manifest_dir() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("could not resolve home directory")?;
        Ok(home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        bail!("`register` only knows where Chrome keeps host manifests on macOS")
    }
}

/// Replaces the unpacked copy wholesale, so a file dropped from
/// `extension/` doesn't linger in the directory Chrome loads.
fn unpack_extension() -> Result<PathBuf> {
    let dir = skagedal_dirs::data_dir(TOOL).join("extension");
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("could not clear {}", dir.display()))?;
    }
    fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    EXTENSION
        .extract(&dir)
        .with_context(|| format!("could not unpack the extension into {}", dir.display()))?;
    Ok(dir)
}

fn write_host_manifest(binary: &Path) -> Result<PathBuf> {
    let dir = manifest_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;

    let path = dir.join(format!("{HOST_NAME}.json"));
    let manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": "chrome-page-notes native messaging host",
        "path": binary,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    });
    let text = serde_json::to_string_pretty(&manifest)? + "\n";
    fs::write(&path, text).with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

/// `dev` holds the checkout to load the extension from instead of the
/// built-in copy (`--dev`, defaulting to [`SOURCE_EXTENSION_DIR`]).
pub fn run(dev: Option<PathBuf>) -> Result<()> {
    let extension_dir = match dev {
        Some(dir) => {
            if !dir.join("manifest.json").is_file() {
                bail!(
                    "{} doesn't look like the extension directory (no manifest.json in it)",
                    dir.display()
                );
            }
            dir
        }
        None => unpack_extension()?,
    };

    // Chrome runs whatever `path` says with no arguments and no working
    // directory of ours, so it has to be an absolute path to this binary.
    let binary = std::env::current_exe()
        .context("could not determine the path to this binary")?
        .canonicalize()
        .context("could not resolve the path to this binary")?;
    let manifest_path = write_host_manifest(&binary)?;

    println!("Extension:     {}", extension_dir.display());
    println!("Host manifest: {}", manifest_path.display());
    println!("  -> {}", binary.display());
    println!();
    println!("To enable the extension in Chrome:");
    println!("  1. Go to chrome://extensions");
    println!("  2. Turn on \"Developer mode\" (top right)");
    println!("  3. Click \"Load unpacked\" and select");
    println!("     {}", extension_dir.display());
    println!("     — or, if it's already loaded, click its reload icon");
    println!();
    println!("Its toolbar icon then shows a badge on pages that have a note,");
    println!("and its popup creates and opens them.");

    let config_path = config::config_path();
    if !config_path.exists() {
        println!();
        println!("No config file yet — create {}", config_path.display());
        println!("with the path to your Obsidian vault:");
        println!();
        println!("  vault_path = \"/path/to/your/vault\"");
    }

    Ok(())
}
