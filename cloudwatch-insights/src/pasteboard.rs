//! Cross-platform clipboard read/write via host utilities.
//!
//!   macOS    → pbcopy / pbpaste
//!   Windows  → clip / powershell Get-Clipboard
//!   Linux    → wl-copy/wl-paste (Wayland) or xclip/xsel (X11)

use std::io::Write;
use std::process::{Command, Stdio};

use thiserror::Error;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct PasteboardError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteboardCommand {
    pub cmd: String,
    pub args: Vec<String>,
}

pub fn copy_command() -> Result<PasteboardCommand, PasteboardError> {
    if cfg!(target_os = "macos") {
        return Ok(PasteboardCommand {
            cmd: "pbcopy".into(),
            args: vec![],
        });
    }
    if cfg!(target_os = "windows") {
        return Ok(PasteboardCommand {
            cmd: "clip".into(),
            args: vec![],
        });
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && has_command("wl-copy") {
        return Ok(PasteboardCommand {
            cmd: "wl-copy".into(),
            args: vec![],
        });
    }
    if has_command("xclip") {
        return Ok(PasteboardCommand {
            cmd: "xclip".into(),
            args: vec!["-selection".into(), "clipboard".into()],
        });
    }
    if has_command("xsel") {
        return Ok(PasteboardCommand {
            cmd: "xsel".into(),
            args: vec!["--clipboard".into(), "--input".into()],
        });
    }
    Err(PasteboardError(
        "no clipboard tool found (install xclip, xsel, or wl-clipboard)".into(),
    ))
}

pub fn paste_command() -> Result<PasteboardCommand, PasteboardError> {
    if cfg!(target_os = "macos") {
        return Ok(PasteboardCommand {
            cmd: "pbpaste".into(),
            args: vec![],
        });
    }
    if cfg!(target_os = "windows") {
        return Ok(PasteboardCommand {
            cmd: "powershell".into(),
            args: vec![
                "-NoProfile".into(),
                "-Command".into(),
                "Get-Clipboard".into(),
            ],
        });
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && has_command("wl-paste") {
        return Ok(PasteboardCommand {
            cmd: "wl-paste".into(),
            args: vec!["--no-newline".into()],
        });
    }
    if has_command("xclip") {
        return Ok(PasteboardCommand {
            cmd: "xclip".into(),
            args: vec!["-selection".into(), "clipboard".into(), "-o".into()],
        });
    }
    if has_command("xsel") {
        return Ok(PasteboardCommand {
            cmd: "xsel".into(),
            args: vec!["--clipboard".into(), "--output".into()],
        });
    }
    Err(PasteboardError(
        "no clipboard tool found (install xclip, xsel, or wl-clipboard)".into(),
    ))
}

pub fn copy_to_pasteboard(text: &str) -> Result<(), PasteboardError> {
    let cmd = copy_command()?;
    let mut child = Command::new(&cmd.cmd)
        .args(&cmd.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PasteboardError(format!("failed to run {}: {e}", cmd.cmd)))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| PasteboardError(format!("failed to write to {}: {e}", cmd.cmd)))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| PasteboardError(format!("failed to wait on {}: {e}", cmd.cmd)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PasteboardError(format!(
            "{} exited with {}: {stderr}",
            cmd.cmd,
            output.status.code().unwrap_or(-1),
        )));
    }
    Ok(())
}

pub fn read_from_pasteboard() -> Result<String, PasteboardError> {
    let cmd = paste_command()?;
    let output = Command::new(&cmd.cmd)
        .args(&cmd.args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| PasteboardError(format!("failed to run {}: {e}", cmd.cmd)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PasteboardError(format!(
            "{} exited with {}: {stderr}",
            cmd.cmd,
            output.status.code().unwrap_or(-1),
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
