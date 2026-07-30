//! Native messaging host for the chrome-page-notes extension.
//!
//! Chrome spawns this process per `chrome.runtime.sendNativeMessage` call
//! and talks to it over stdin/stdout using length-prefixed JSON: each
//! message is preceded by its byte length as a 32-bit integer in native
//! byte order. See
//! <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging>.

mod cli;
mod config;
mod notes;

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Request {
    /// Popup opened for this tab: log it, and report whether a note exists.
    Activated { url: String },
    /// Background service worker saw a tab navigate to (or focus on) a URL:
    /// log it, and report whether a note exists (used to set the toolbar
    /// badge for that tab).
    UrlVisited { url: String },
    /// "Create note" button in the popup.
    CreateNote { url: String },
    /// "Open in Obsidian" button in the popup.
    OpenNote { path: String },
}

fn log_path() -> PathBuf {
    let base = match std::env::var("SKAGEDAL_TOOLS_HOME") {
        Ok(p) => PathBuf::from(p),
        Err(_) => dirs::home_dir()
            .expect("could not resolve home directory")
            .join(".skagedal-tools"),
    };
    base.join("chrome-page-notes").join("host.log")
}

fn log_line(line: &str) -> io::Result<()> {
    let path = log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")
}

fn read_message(stdin: &mut impl Read) -> io::Result<Option<serde_json::Value>> {
    let mut len_buf = [0u8; 4];
    if let Err(e) = stdin.read_exact(&mut len_buf) {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(e);
    }
    let len = u32::from_ne_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stdin.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf)?))
}

fn write_message(stdout: &mut impl Write, value: &serde_json::Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    stdout.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    stdout.write_all(&bytes)?;
    stdout.flush()
}

/// Looks up (or creates) the note for `url` and builds the popup-facing
/// response. `note_path` returning `None` means an unsupported scheme
/// (anything but `https`), which isn't an error - just nothing to show.
fn note_response(cfg: &Result<config::Config>, url: &str, create_if_missing: bool) -> serde_json::Value {
    let cfg = match cfg {
        Ok(cfg) => cfg,
        Err(e) => return json!({"status": "error", "message": e.to_string()}),
    };

    let Some(path) = notes::note_path(url, &cfg.folder) else {
        return json!({"status": "ok", "note": null});
    };

    let result = if create_if_missing {
        notes::create(&cfg.obsidian_binary, &cfg.vault, &path, &notes::initial_content(url))
    } else {
        notes::lookup(&cfg.obsidian_binary, &cfg.vault, &path)
    };

    match result {
        Ok(note) => json!({
            "status": "ok",
            "note": {"exists": note.exists, "path": note.path, "content": note.content},
        }),
        Err(e) => json!({"status": "error", "message": e.to_string()}),
    }
}

fn handle(request: Request, cfg: &Result<config::Config>) -> serde_json::Value {
    match request {
        Request::UrlVisited { url } => note_response(cfg, &url, false),
        Request::Activated { url } => note_response(cfg, &url, false),
        Request::CreateNote { url } => note_response(cfg, &url, true),
        Request::OpenNote { path } => {
            let cfg = match cfg {
                Ok(cfg) => cfg,
                Err(e) => return json!({"status": "error", "message": e.to_string()}),
            };
            match notes::open(&cfg.obsidian_binary, &cfg.vault, &path) {
                Ok(()) => json!({"status": "ok"}),
                Err(e) => json!({"status": "error", "message": e.to_string()}),
            }
        }
    }
}

fn main() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    let cfg = config::load();

    loop {
        match read_message(&mut stdin) {
            Ok(Some(value)) => {
                let timestamp = chrono::Utc::now().to_rfc3339();
                let _ = log_line(&format!("{timestamp} {value}"));

                let response = match serde_json::from_value::<Request>(value) {
                    Ok(request) => handle(request, &cfg),
                    Err(e) => json!({"status": "error", "message": format!("invalid request: {e}")}),
                };
                let _ = write_message(&mut stdout, &response);
            }
            Ok(None) => break,
            Err(e) => {
                let timestamp = chrono::Utc::now().to_rfc3339();
                let _ = log_line(&format!("{timestamp} error: {e}"));
                break;
            }
        }
    }
}
