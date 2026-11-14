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
//!   POST /api/answers   `{answers: [{key, comment, category}]}`, only for
//!                       `kontoutdrag ask`: saved as comments, printed to
//!                       stdout as JSON, and then the command exits
//!
//! The document is rebuilt when a statement, table, marks file, budget file
//! or the settings change on disk. A rebuild that fails — a marks file saved
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

/// 💰, drawn by build.rs when appicon-generator was there to draw it.
#[cfg(kontoutdrag_icon)]
const ICON: Option<&[u8]> = Some(include_bytes!(concat!(env!("OUT_DIR"), "/icon.png")));
#[cfg(not(kontoutdrag_icon))]
const ICON: Option<&[u8]> = None;

/// How to rebuild the document, and what it is built from.
pub struct Source {
    pub build: Box<dyn Fn() -> Result<Built> + Send + Sync>,
    pub watched: Vec<PathBuf>,
    pub comments: PathBuf,
    /// Whether this is `kontoutdrag ask`, which ends when the answers come.
    pub asking: bool,
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

#[derive(Deserialize)]
struct AnswersRequest {
    answers: Vec<Answer>,
}

#[derive(Deserialize)]
struct Answer {
    key: String,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    category: String,
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
    webview_shell::window::open(&url, "kontoutdrag", (1200.0, 820.0), ICON)
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
        ("POST", "/api/answers") if state.source.asking => {
            save_answers(state, &request.body, stream)?
        }
        ("GET", "/icon.png") => match ICON {
            Some(png) => server::send_png(stream, png)?,
            None => server::send_status(stream, 404, "Not Found")?,
        },
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

/// Save every answer as a comment, print them all, and end the command, so
/// whoever ran `kontoutdrag ask` gets them as its output.
fn save_answers(state: &State, body: &[u8], stream: &mut TcpStream) -> io::Result<()> {
    let request: AnswersRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return send_error(stream, 400, &format!("bad request: {e}")),
    };
    let subjects = state.current.lock().unwrap().subjects.clone();
    let _writing = state.writing.lock().unwrap();
    let now = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let mut printed = Vec::new();
    for answer in &request.answers {
        if answer.comment.trim().is_empty() && answer.category.trim().is_empty() {
            continue;
        }
        let Some(subject) = subjects.get(&answer.key).cloned() else {
            return send_error(
                stream,
                404,
                &format!("no transaction has the key {}", answer.key),
            );
        };
        let path = &state.source.comments;
        if let Err(e) = comments::answer(
            path,
            subject.clone(),
            &answer.comment,
            &answer.category,
            &now,
        ) {
            return send_error(stream, 500, &format!("{e:#}"));
        }
        printed.push(comments::Comment {
            subject,
            comment: answer.comment.clone(),
            category: Some(answer.category.trim().to_string()).filter(|c| !c.is_empty()),
            updated: now.clone(),
        });
    }
    let out = json!({ "answers": printed });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    io::stdout().flush()?;
    server::send_json(stream, r#"{"ok":true}"#)?;
    // Give the page a moment to read the reply before the server goes.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(300));
        std::process::exit(0);
    });
    Ok(())
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

/// Changes whenever any of the files is written, created or removed. A
/// directory counts as every file directly inside it.
fn fingerprint(files: &[PathBuf]) -> String {
    let mut hasher = DefaultHasher::new();
    for file in files {
        stamp(file).hash(&mut hasher);
        if let Ok(entries) = std::fs::read_dir(file) {
            let mut inside: Vec<PathBuf> =
                entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            inside.sort();
            for path in inside {
                path.hash(&mut hasher);
                stamp(&path).hash(&mut hasher);
            }
        }
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
