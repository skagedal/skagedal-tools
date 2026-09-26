//! The window for `kontoutdrag view`: the React app from `browser/dist/`,
//! embedded at build time and served through `webview_shell`. Compiled in
//! only with the `web` feature.
//!
//! Endpoints:
//!   GET  /api/data      the document from `commands::view::build`
//!   GET  /api/version   fingerprints of the rule files and of the comments
//!                       file, so the page can poll and reload what changed
//!   GET  /api/comments  the comments file
//!   POST /api/comment   `{key, comment}`: set, or clear when blank
//!
//! The document is rebuilt when a statement, table, marks file or the
//! settings change on disk. A rebuild that fails — a marks file saved
//! half-edited, say — keeps the last good document and reports the error.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_json::json;
use webview_shell::server::{self, Request};

use crate::commands::view::Built;
use crate::comments::{self, Subject};

static WEB_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/browser/dist");

/// How to rebuild the document, and what it is built from.
pub struct Source {
    pub build: Box<dyn Fn() -> Result<Built> + Send + Sync>,
    pub watched: Vec<PathBuf>,
    pub comments: PathBuf,
}

struct State {
    source: Source,
    current: Mutex<Current>,
    /// Serialises writes to the comments file.
    writing: Mutex<()>,
}

struct Current {
    fingerprint: String,
    json: String,
    subjects: HashMap<String, Subject>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct CommentRequest {
    key: String,
    comment: String,
}

pub fn run(source: Source, first: Built, serve_only: bool) -> Result<()> {
    let state = Arc::new(State {
        current: Mutex::new(Current {
            fingerprint: fingerprint(&source.watched),
            json: first.json,
            subjects: first.subjects,
            error: None,
        }),
        source,
        writing: Mutex::new(()),
    });
    let port = server::start(
        &WEB_DIST,
        Arc::new(move |request, stream| handle(&state, request, stream)),
    )
    .context("starting HTTP server")?;
    let url = format!("http://127.0.0.1:{port}/");

    if serve_only {
        eprintln!("kontoutdrag view listening at {url} — Ctrl-C to stop");
        loop {
            std::thread::park();
        }
    }
    webview_shell::window::open(&url, "kontoutdrag", (1200.0, 820.0))
}

fn handle(state: &State, request: &Request, stream: &mut TcpStream) -> io::Result<bool> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/data") => {
            refresh(state);
            let json = state.current.lock().unwrap().json.clone();
            server::send_json(stream, &json)?;
        }
        ("GET", "/api/version") => {
            refresh(state);
            let current = state.current.lock().unwrap();
            let body = json!({
                "data": current.fingerprint,
                "comments": fingerprint(std::slice::from_ref(&state.source.comments)),
                "error": current.error,
            });
            server::send_json(stream, &body.to_string())?;
        }
        ("GET", "/api/comments") => match comments::load(&state.source.comments) {
            Ok(all) => server::send_json(stream, &json!({ "comments": all }).to_string())?,
            Err(e) => send_error(stream, 500, &format!("{e:#}"))?,
        },
        ("POST", "/api/comment") => save_comment(state, &request.body, stream)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn save_comment(state: &State, body: &[u8], stream: &mut TcpStream) -> io::Result<()> {
    let request: CommentRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return send_error(stream, 400, &format!("bad request: {e}")),
    };
    let subject = state
        .current
        .lock()
        .unwrap()
        .subjects
        .get(&request.key)
        .cloned();
    let Some(subject) = subject else {
        return send_error(stream, 404, "no transaction has that key");
    };
    let _writing = state.writing.lock().unwrap();
    let now = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    match comments::upsert(&state.source.comments, subject, &request.comment, &now) {
        Ok(()) => server::send_json(stream, r#"{"ok":true}"#),
        Err(e) => send_error(stream, 500, &format!("{e:#}")),
    }
}

/// Rebuild the document if anything it is built from has changed.
fn refresh(state: &State) {
    let now = fingerprint(&state.source.watched);
    let mut current = state.current.lock().unwrap();
    if current.fingerprint == now {
        return;
    }
    current.fingerprint = now;
    match (state.source.build)() {
        Ok(built) => {
            current.json = built.json;
            current.subjects = built.subjects;
            current.error = None;
        }
        Err(e) => current.error = Some(format!("{e:#}")),
    }
}

/// Changes whenever any of the files is written, created or removed.
fn fingerprint(files: &[PathBuf]) -> String {
    let mut hasher = DefaultHasher::new();
    for file in files {
        stamp(file).hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn stamp(file: &Path) -> Option<(u128, u64)> {
    let meta = std::fs::metadata(file).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some((modified.as_nanos(), meta.len()))
}

fn send_error(stream: &mut TcpStream, code: u16, message: &str) -> io::Result<()> {
    let body = json!({ "error": message }).to_string();
    write!(
        stream,
        "HTTP/1.1 {code} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
