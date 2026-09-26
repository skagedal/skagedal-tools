//! The window for `kontoutdrag view`: the React app from `browser/dist/`,
//! embedded at build time and served through `webview_shell` beside one
//! endpoint, `/api/data`. Compiled in only with the `web` feature.

use std::sync::Arc;

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};
use webview_shell::server;

static WEB_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/browser/dist");

pub fn run(data: String, serve_only: bool) -> Result<()> {
    let data = Arc::new(data);
    let port = server::start(
        &WEB_DIST,
        Arc::new(move |path, stream| {
            if path != "/api/data" {
                return Ok(false);
            }
            server::send_json(stream, &data)?;
            Ok(true)
        }),
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
