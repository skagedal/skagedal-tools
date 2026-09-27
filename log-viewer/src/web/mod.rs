//! Webview-embedded React front-end. Compiled in only when the `web` feature
//! is on.
//!
//! Embeds the React app from `browser/web/dist/` into the Rust binary, serves
//! it through `webview_shell` (with the same `/api/meta` and `/api/stream` SSE
//! contract the in-tree TS browser server uses), and opens a webview on it.
//! The React code is consumed verbatim.

mod server;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::source::EntryStream;
use crate::triggers::TriggerRuntime;

use self::server::ServerState;

pub fn run(
    config: Config,
    source_label: String,
    stream: EntryStream,
    triggers: TriggerRuntime,
) -> Result<()> {
    let state = Arc::new(ServerState::new(&config, source_label.clone()));
    let handler_state = state.clone();
    let port = webview_shell::server::start(
        &server::WEB_DIST,
        Arc::new(move |request, stream| server::handle(&handler_state, request, stream)),
    )
    .context("starting HTTP server")?;
    let url = format!("http://127.0.0.1:{port}/");

    spawn_consumer(stream, triggers, state.clone());

    eprintln!("log-viewer (web) listening at {url}");
    eprintln!("  source: {source_label}");

    webview_shell::window::open(
        &url,
        &format!("log-viewer — {source_label}"),
        (1100.0, 720.0),
        None,
    )
}

fn spawn_consumer(stream: EntryStream, mut triggers: TriggerRuntime, state: Arc<ServerState>) {
    thread::spawn(move || {
        loop {
            let new = stream.drain();
            if !new.is_empty() {
                if !triggers.is_empty() {
                    for e in &new {
                        triggers.handle(e);
                    }
                }
                for entry in new {
                    state.push_entry(&entry);
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
}
