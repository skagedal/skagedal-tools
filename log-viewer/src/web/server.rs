//! The log-viewer API, served by `webview_shell::server` beside the
//! embedded React app.
//!
//! Endpoints:
//!   GET /api/meta     -> JSON: { sourceLabel, config: { fields, defaultField } }
//!   GET /api/stream   -> text/event-stream: replay then live entries
//!
//! The wire format mirrors the in-tree TS browser server exactly (see
//! `browser/src/browser/server.ts`) so the React app under `browser/web/`
//! works without modification.

use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use include_dir::{Dir, include_dir};
use serde_json::json;
use webview_shell::server;

use crate::config::Config;
use crate::entry::Entry;

pub static WEB_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/browser/web/dist");

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

/// Routes the API; everything else falls through to the embedded app.
pub fn handle(state: &ServerState, path: &str, stream: &mut TcpStream) -> io::Result<bool> {
    match path {
        "/api/meta" => server::send_json(stream, &state.meta_json)?,
        "/api/stream" => handle_sse(stream, state)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn handle_sse(stream: &mut TcpStream, state: &ServerState) -> io::Result<()> {
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
        write_event(stream, "entry", payload)?;
    }
    if state.ended.load(Ordering::SeqCst) {
        write_event(stream, "end", "{}")?;
        return Ok(());
    }

    while let Ok(payload) = rx.recv() {
        if write_event(stream, "entry", &payload).is_err() {
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
