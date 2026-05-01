//! Browser-GUI front-end. Compiled in only when the `web` feature is on.
//!
//! This mode embeds the React app from `log-viewer/web/dist` into the Rust
//! binary, serves it from a localhost HTTP server (with the same `/api/meta`
//! and `/api/stream` SSE contract the TS browser mode uses), and opens a
//! [wry] webview pointing at that URL. The React code is consumed verbatim.
//!
//! The HTTP server is hand-rolled (~250 lines, no axum/hyper) because all we
//! need is two GET endpoints and SSE on a single localhost port.

mod server;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use crate::config::Config;
use crate::source::EntryStream;
use crate::triggers::TriggerRuntime;

use self::server::{ServerHandle, ServerState};

pub fn run(
    config: Config,
    source_label: String,
    stream: EntryStream,
    triggers: TriggerRuntime,
) -> Result<()> {
    let state = Arc::new(ServerState::new(&config, source_label.clone()));
    let handle = ServerHandle::start(state.clone()).context("starting HTTP server")?;
    let url = format!("http://127.0.0.1:{}/", handle.port());

    spawn_consumer(stream, triggers, state.clone());

    eprintln!("rust-log-viewer (web) listening at {url}");
    eprintln!("  source: {source_label}");

    open_window(&url, &source_label)
}

fn spawn_consumer(
    stream: EntryStream,
    mut triggers: TriggerRuntime,
    state: Arc<ServerState>,
) {
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

fn open_window(url: &str, source_label: &str) -> Result<()> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(format!("rust-log-viewer — {source_label}"))
        .with_inner_size(LogicalSize::new(1100.0, 720.0))
        .build(&event_loop)
        .map_err(|e| anyhow::anyhow!("creating window: {e}"))?;
    let _webview = WebViewBuilder::new()
        .with_url(url)
        .build(&window)
        .map_err(|e| anyhow::anyhow!("creating webview: {e}"))?;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}
