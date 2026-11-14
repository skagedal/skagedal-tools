//! The window: the React app from `browser/dist/`, embedded at build time
//! and served through `webview_shell`. Compiled in only with the `web`
//! feature. Nothing here writes to a calendar; the sync it may run is
//! configured to only read.
//!
//! Endpoints:
//!   GET /api/calendars                      the calendars, in order
//!   GET /api/events/<from>/<to>/<i,j,…>     the occurrences of the calendars
//!                                           at those indices, days inclusive
//!   GET /api/version                        changes when the store does, so
//!                                           the page can poll and reload;
//!                                           with the sync's status
//!   POST /api/sync                          run the sync command now

use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use include_dir::{Dir, include_dir};
use serde_json::json;
use webview_shell::server::{self, Request};

use crate::agenda::Agenda;
use crate::sync::Syncer;

static WEB_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/browser/dist");

#[cfg(calendar_view_icon)]
const ICON: Option<&[u8]> = Some(include_bytes!(concat!(env!("OUT_DIR"), "/icon.png")));
#[cfg(not(calendar_view_icon))]
const ICON: Option<&[u8]> = None;

/// A longer range is refused, so a stray request cannot expand years of series.
const MAX_DAYS: i64 = 62;

pub fn run(agenda: Agenda, syncer: Arc<Syncer>, serve_only: bool) -> Result<()> {
    let agenda = Arc::new(agenda);
    let port = server::start(
        &WEB_DIST,
        Arc::new(move |request, stream| handle(&agenda, &syncer, request, stream)),
    )
    .context("starting HTTP server")?;
    let url = format!("http://127.0.0.1:{port}/");

    if serve_only {
        eprintln!("calendar-view listening at {url} — Ctrl-C to stop");
        loop {
            std::thread::park();
        }
    }
    webview_shell::window::open(&url, "calendar-view", (1200.0, 860.0), ICON)
}

fn handle(
    agenda: &Agenda,
    syncer: &Syncer,
    request: &Request,
    stream: &mut TcpStream,
) -> io::Result<bool> {
    let path = request.path.as_str();
    if request.method == "POST" && path == "/api/sync" {
        syncer.ask();
        server::send_json(stream, r#"{"ok":true}"#)?;
        return Ok(true);
    }
    if request.method != "GET" {
        return Ok(false);
    }
    if path == "/api/calendars" {
        let body = serde_json::to_string(&agenda.calendars).unwrap_or_default();
        server::send_json(stream, &body)?;
    } else if path == "/api/version" {
        let body = json!({ "version": agenda.version(), "sync": syncer.status() });
        server::send_json(stream, &body.to_string())?;
    } else if let Some(rest) = path.strip_prefix("/api/events/") {
        match parse_events(rest) {
            Ok((from, to, which)) => {
                let overview = agenda.overview(&which, from, to);
                server::send_json(
                    stream,
                    &serde_json::to_string(&overview).unwrap_or_default(),
                )?;
            }
            Err(message) => send_error(stream, 400, &message)?,
        }
    } else if path == "/icon.png" {
        match ICON {
            Some(png) => server::send_png(stream, png)?,
            None => server::send_status(stream, 404, "Not Found")?,
        }
    } else {
        return Ok(false);
    }
    Ok(true)
}

fn parse_events(rest: &str) -> Result<(NaiveDate, NaiveDate, Vec<usize>), String> {
    let mut parts = rest.splitn(3, '/');
    let day = |s: Option<&str>| {
        s.and_then(|s| s.parse::<NaiveDate>().ok())
            .ok_or_else(|| "expected /api/events/<from>/<to>/<calendars>".to_string())
    };
    let from = day(parts.next())?;
    let to = day(parts.next())?;
    if to < from || (to - from).num_days() > MAX_DAYS {
        return Err(format!(
            "the range must run forwards, {MAX_DAYS} days at most"
        ));
    }
    let which = parts
        .next()
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| format!("not a calendar index: {s}"))
        })
        .collect::<Result<_, _>>()?;
    Ok((from, to, which))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_week_of_two_calendars() {
        let (from, to, which) = parse_events("2026-11-16/2026-11-22/0,3").unwrap();
        assert_eq!(from.to_string(), "2026-11-16");
        assert_eq!(to.to_string(), "2026-11-22");
        assert_eq!(which, [0, 3]);
    }

    #[test]
    fn refuses_a_backwards_or_long_range() {
        assert!(parse_events("2026-11-22/2026-11-16/0").is_err());
        assert!(parse_events("2026-01-01/2026-12-31/0").is_err());
    }

    #[test]
    fn no_calendars_is_no_events() {
        let (_, _, which) = parse_events("2026-11-16/2026-11-22/").unwrap();
        assert!(which.is_empty());
    }
}
