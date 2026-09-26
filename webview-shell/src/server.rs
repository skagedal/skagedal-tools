//! A hand-rolled localhost HTTP server, one thread per connection.
//!
//! It serves the embedded app for any path the tool's own handler does not
//! claim. The tool's handler sees every request first and returns whether it
//! answered; a handler may hold the stream open, as server-sent events do.
//! No axum or hyper: a few endpoints on one local port do not need them.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use include_dir::Dir;

/// One HTTP request, as much of it as a local API needs.
#[derive(Debug, Default)]
pub struct Request {
    /// `GET`, `PUT` and so on, as sent.
    pub method: String,
    /// The path, with any query string removed.
    pub path: String,
    pub body: Vec<u8>,
}

/// Answers the request and returns true, or returns false to fall through
/// to the embedded assets.
pub type Handler = dyn Fn(&Request, &mut TcpStream) -> io::Result<bool> + Send + Sync;

/// A request body larger than this is refused rather than read.
const MAX_BODY: usize = 16 * 1024 * 1024;

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
    let request = read_request(&mut stream)?;
    if handler(&request, &mut stream)? {
        return Ok(());
    }
    serve_asset(&mut stream, assets, &request.path)
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let (method, path) =
        parse_request_line(&line).unwrap_or_else(|| ("GET".to_string(), "/".to_string()));
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    if length > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, body })
}

fn parse_request_line(request_line: &str) -> Option<(String, String)> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let raw = parts.next()?;
    let path = raw.split_once('?').map(|(p, _)| p).unwrap_or(raw);
    Some((method, path.to_string()))
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
    fn parse_request_line_strips_query() {
        assert_eq!(
            parse_request_line("GET /api/meta?x=1 HTTP/1.1\r\n"),
            Some(("GET".to_string(), "/api/meta".to_string()))
        );
        assert_eq!(
            parse_request_line("PUT / HTTP/1.1\r\n"),
            Some(("PUT".to_string(), "/".to_string()))
        );
        assert_eq!(parse_request_line("garbage"), None);
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

    /// A real round trip over a socket: the handler sees method, path and
    /// body, and a path it declines falls through to the assets.
    #[test]
    fn hands_the_body_to_the_handler() {
        static EMPTY: Dir<'static> = Dir::new("", &[]);
        let port = start(
            &EMPTY,
            Arc::new(|req: &Request, stream: &mut TcpStream| {
                if req.path != "/echo" {
                    return Ok(false);
                }
                let body = format!("{} {}", req.method, String::from_utf8_lossy(&req.body));
                send_json(stream, &body)?;
                Ok(true)
            }),
        )
        .unwrap();
        let ask = |raw: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(raw.as_bytes()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };
        let echoed = ask("PUT /echo HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello");
        assert!(echoed.ends_with("PUT hello"), "{echoed}");
        let missing = ask("GET /nothing HTTP/1.1\r\n\r\n");
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    }
}
