//! Hand-rolled HTTP/SSE server. One thread per accepted connection.
//!
//! Endpoints:
//!   GET /             -> embedded React app (index.html)
//!   GET /assets/<f>   -> embedded React asset
//!   GET /<other>      -> falls through to embedded asset lookup, then 404
//!   GET /api/meta     -> JSON: { sourceLabel, config: { fields, defaultField } }
//!   GET /api/stream   -> text/event-stream: replay then live entries
//!
//! The wire format mirrors the in-tree TS browser server exactly (see
//! `browser/src/browser/server.ts`) so the React app under `browser/web/`
//! works without modification.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};
use serde_json::json;

use crate::config::Config;
use crate::entry::Entry;

static WEB_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/browser/web/dist");

/// Pre-rendered SSE payload (just the JSON body — the `event:`/`data:` framing
/// is added on write).
type Payload = String;

pub struct ServerState {
    meta_json: String,
    replay: Mutex<Vec<Payload>>,
    subscribers: Mutex<Vec<Sender<Payload>>>,
    next_id: Mutex<u64>,
    ended: AtomicBool,
}

impl ServerState {
    pub fn new(config: &Config, source_label: String) -> Self {
        Self {
            meta_json: meta_payload(config, &source_label),
            replay: Mutex::new(Vec::new()),
            subscribers: Mutex::new(Vec::new()),
            next_id: Mutex::new(0),
            ended: AtomicBool::new(false),
        }
    }

    pub fn push_entry(&self, entry: &Entry) {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };
        let payload = entry_payload(entry, id);
        // Order matters: replay then subscribers, both held together so a
        // joining client either sees this entry in its replay snapshot or
        // gets it on its channel — never both, never neither.
        let mut replay = self.replay.lock().unwrap();
        let mut subs = self.subscribers.lock().unwrap();
        replay.push(payload.clone());
        subs.retain(|tx| tx.send(payload.clone()).is_ok());
    }
}

fn meta_payload(config: &Config, source_label: &str) -> String {
    let fields: Vec<_> = config
        .fields
        .iter()
        .map(|f| json!({ "name": f.name, "from": f.from }))
        .collect();
    json!({
        "sourceLabel": source_label,
        "config": {
            "fields": fields,
            "defaultField": config.default_field,
        }
    })
    .to_string()
}

fn entry_payload(entry: &Entry, id: u64) -> String {
    json!({
        "id": id,
        "raw": entry.raw,
        "data": entry.value,
        "wrapped": entry.wrapped,
    })
    .to_string()
}

pub struct ServerHandle {
    port: u16,
}

impl ServerHandle {
    pub fn start(state: Arc<ServerState>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("binding 127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        thread::spawn(move || accept_loop(listener, state));
        Ok(Self { port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

fn accept_loop(listener: TcpListener, state: Arc<ServerState>) {
    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        let s = state.clone();
        thread::spawn(move || {
            if let Err(err) = handle_connection(stream, &s) {
                // Connection-level errors are common (clients disconnect);
                // a debug log here would be noisy.
                let _ = err;
            }
        });
    }
}

fn handle_connection(mut stream: TcpStream, state: &ServerState) -> io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let path = read_request_path(&mut stream)?;
    match path.as_str() {
        "/api/meta" => send_json(&mut stream, &state.meta_json),
        "/api/stream" => handle_sse(stream, state),
        other => serve_asset(&mut stream, other),
    }
}

fn read_request_path(stream: &mut TcpStream) -> io::Result<String> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = parse_request_path(&line).unwrap_or_else(|| "/".to_string());
    // Drain the rest of the headers so the kernel buffer doesn't keep
    // requests pending on close.
    loop {
        let mut hdr = String::new();
        let n = reader.read_line(&mut hdr)?;
        if n == 0 || hdr == "\r\n" || hdr == "\n" {
            break;
        }
    }
    Ok(path)
}

fn parse_request_path(request_line: &str) -> Option<String> {
    let mut parts = request_line.split_whitespace();
    let _method = parts.next()?;
    let raw = parts.next()?;
    Some(strip_query(raw).to_string())
}

fn strip_query(s: &str) -> &str {
    s.split_once('?').map(|(p, _)| p).unwrap_or(s)
}

fn send_status(stream: &mut TcpStream, code: u16, msg: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} {msg}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()
}

fn send_json(stream: &mut TcpStream, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn serve_asset(stream: &mut TcpStream, path: &str) -> io::Result<()> {
    let lookup = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    let Some(file) = WEB_DIST.get_file(lookup) else {
        return send_status(stream, 404, "Not Found");
    };
    let body = file.contents();
    let mime = guess_mime(lookup);
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn guess_mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

fn handle_sse(mut stream: TcpStream, state: &ServerState) -> io::Result<()> {
    // Long-lived: drop the read timeout, but cap writes so a wedged client
    // doesn't block forever.
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n"
    )?;
    stream.flush()?;

    let (tx, rx): (Sender<Payload>, Receiver<Payload>) = channel();
    let snapshot: Vec<Payload> = {
        // Same lock order as the producer: replay first, then subscribers.
        let replay = state.replay.lock().unwrap();
        let mut subs = state.subscribers.lock().unwrap();
        subs.push(tx);
        replay.clone()
    };

    for payload in &snapshot {
        write_event(&mut stream, "entry", payload)?;
    }
    if state.ended.load(Ordering::SeqCst) {
        write_event(&mut stream, "end", "{}")?;
        return Ok(());
    }

    while let Ok(payload) = rx.recv() {
        if write_event(&mut stream, "entry", &payload).is_err() {
            break;
        }
    }
    Ok(())
}

fn write_event(stream: &mut TcpStream, event: &str, data: &str) -> io::Result<()> {
    write!(stream, "event: {event}\ndata: {data}\n\n")?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn meta_payload_matches_react_contract() {
        let cfg = Config {
            default_field: "msg".into(),
            ..Config::default()
        };
        let json = meta_payload(&cfg, "stdin");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["sourceLabel"], "stdin");
        assert_eq!(value["config"]["defaultField"], "msg");
        // First default field is "time" with a list of candidate keys.
        assert_eq!(value["config"]["fields"][0]["name"], "time");
        assert!(value["config"]["fields"][0]["from"].is_array());
    }

    #[test]
    fn entry_payload_has_id_raw_data_wrapped() {
        let entry = Entry::parse(r#"{"a":1}"#, "message");
        let json = entry_payload(&entry, 7);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["raw"], r#"{"a":1}"#);
        assert_eq!(value["data"]["a"], 1);
        assert_eq!(value["wrapped"], false);
    }

    #[test]
    fn entry_payload_marks_non_json_as_wrapped() {
        let entry = Entry::parse("plain text", "message");
        let json = entry_payload(&entry, 0);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["wrapped"], true);
        assert_eq!(value["data"]["message"], "plain text");
    }

    #[test]
    fn parse_request_path_strips_query() {
        assert_eq!(
            parse_request_path("GET /api/meta?x=1 HTTP/1.1\r\n").as_deref(),
            Some("/api/meta")
        );
        assert_eq!(
            parse_request_path("GET / HTTP/1.1\r\n").as_deref(),
            Some("/")
        );
        assert_eq!(parse_request_path("garbage"), None);
    }

    #[test]
    fn guess_mime_for_common_extensions() {
        assert_eq!(guess_mime("index.html"), "text/html; charset=utf-8");
        assert_eq!(guess_mime("main.js"), "application/javascript; charset=utf-8");
        assert_eq!(guess_mime("app.css"), "text/css; charset=utf-8");
        assert_eq!(guess_mime("logo.svg"), "image/svg+xml");
        assert_eq!(guess_mime("unknown"), "application/octet-stream");
    }

    #[test]
    fn push_entry_assigns_monotonic_ids_and_buffers_replay() {
        let state = ServerState::new(&Config::default(), "test".into());
        state.push_entry(&Entry::parse(r#"{"a":1}"#, "message"));
        state.push_entry(&Entry::parse(r#"{"a":2}"#, "message"));
        let replay = state.replay.lock().unwrap();
        assert_eq!(replay.len(), 2);
        let v0: serde_json::Value = serde_json::from_str(&replay[0]).unwrap();
        let v1: serde_json::Value = serde_json::from_str(&replay[1]).unwrap();
        assert_eq!(v0["id"], 0);
        assert_eq!(v1["id"], 1);
    }
}

