//! A hand-rolled localhost HTTP server, one thread per connection.
//!
//! It serves the embedded app for any path the tool's own handler does not
//! claim. The tool's handler sees every request first and returns whether it
//! answered; a handler may hold the stream open, as server-sent events do.
//! No axum or hyper: a few GET endpoints on one local port do not need them.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use include_dir::Dir;

/// Answers the request for `path` (query string removed) and returns true,
/// or returns false to fall through to the embedded assets.
pub type Handler = dyn Fn(&str, &mut TcpStream) -> io::Result<bool> + Send + Sync;

/// Start serving on an ephemeral port on 127.0.0.1 and return the port.
/// The server runs until the process exits.
pub fn start(assets: &'static Dir<'static>, handler: Arc<Handler>) -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("binding 127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let handler = handler.clone();
            // Connection-level errors are routine (clients disconnect), so
            // they are dropped rather than logged.
            thread::spawn(move || {
                let _ = handle_connection(stream, assets, &*handler);
            });
        }
    });
    Ok(port)
}

fn handle_connection(
    mut stream: TcpStream,
    assets: &Dir<'static>,
    handler: &Handler,
) -> io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let path = read_request_path(&mut stream)?;
    if handler(&path, &mut stream)? {
        return Ok(());
    }
    serve_asset(&mut stream, assets, &path)
}

fn read_request_path(stream: &mut TcpStream) -> io::Result<String> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = parse_request_path(&line).unwrap_or_else(|| "/".to_string());
    // Drain the headers so the request does not sit in the kernel buffer.
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    Ok(path)
}

fn parse_request_path(request_line: &str) -> Option<String> {
    let mut parts = request_line.split_whitespace();
    let _method = parts.next()?;
    let raw = parts.next()?;
    Some(
        raw.split_once('?')
            .map(|(p, _)| p)
            .unwrap_or(raw)
            .to_string(),
    )
}

pub fn send_status(stream: &mut TcpStream, code: u16, msg: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} {msg}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()
}

pub fn send_json(stream: &mut TcpStream, body: &str) -> io::Result<()> {
    send_body(stream, "application/json", body.as_bytes())
}

fn send_body(stream: &mut TcpStream, mime: &str, body: &[u8]) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn serve_asset(stream: &mut TcpStream, assets: &Dir<'static>, path: &str) -> io::Result<()> {
    let lookup = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    match assets.get_file(lookup) {
        Some(file) => send_body(stream, guess_mime(lookup), file.contents()),
        None => send_status(stream, 404, "Not Found"),
    }
}

fn guess_mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            guess_mime("main.js"),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(guess_mime("app.css"), "text/css; charset=utf-8");
        assert_eq!(guess_mime("logo.svg"), "image/svg+xml");
        assert_eq!(guess_mime("unknown"), "application/octet-stream");
    }
}
