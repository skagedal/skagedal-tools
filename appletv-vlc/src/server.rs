//! Serving a movie, and optionally its subtitles, over HTTP, with byte
//! ranges so VLC can seek.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result, anyhow};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// What a path segment may contain unencoded, as Python's `urllib.parse.quote`.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

const CHUNK: usize = 256 * 1024;

/// A file being served: the movie or its subtitles.
pub struct ServedFile {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub content_type: &'static str,
}

impl ServedFile {
    pub fn open(path: &Path) -> Result<Self> {
        let path = path
            .canonicalize()
            .with_context(|| format!("no such file: {}", path.display()))?;
        let metadata = path.metadata()?;
        if !metadata.is_file() {
            return Err(anyhow!("not a file: {}", path.display()));
        }
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(ServedFile {
            content_type: content_type(&path),
            path,
            file_name,
            size: metadata.len(),
        })
    }
}

/// The files being served, all under one secret first path segment, so the
/// open port does not expose them to everything else on the network.
pub struct Site {
    pub token: String,
    /// The movie first, then any subtitles.
    pub files: Vec<ServedFile>,
}

impl Site {
    /// The path `file` is served at.
    pub fn url_path(&self, file: &ServedFile) -> String {
        format!(
            "/{}/{}",
            self.token,
            utf8_percent_encode(&file.file_name, PATH_SEGMENT)
        )
    }

    /// The file a request path asks for. Anything under the token that names
    /// none of the files gets the movie, as before there were subtitles to
    /// serve.
    fn file_for(&self, path: &str) -> Option<&ServedFile> {
        let rest = path.strip_prefix(&format!("/{}", self.token))?;
        let name = percent_decode_str(rest.trim_start_matches('/')).decode_utf8_lossy();
        self.files
            .iter()
            .find(|file| file.file_name == name)
            .or(self.files.first())
    }
}

/// Start serving `site` on `port`, on every interface.
pub fn serve(site: Site, port: u16) -> Result<thread::JoinHandle<()>> {
    let server = Server::http(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
        .map_err(|error| anyhow!("could not listen on port {port}: {error}"))?;
    for file in &site.files {
        eprintln!(
            "  [http] serving {} ({:.1} MiB) on port {port}",
            file.file_name,
            file.size as f64 / 1_048_576.0
        );
    }
    let site = Arc::new(site);
    Ok(thread::spawn(move || {
        for request in server.incoming_requests() {
            let site = Arc::clone(&site);
            thread::spawn(move || handle(&site, request));
        }
    }))
}

fn handle(site: &Site, request: Request) {
    let range_header = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .map(|header| header.value.as_str().to_string());
    let line = format!("{} {}", request.method(), request.url());
    let file = site.file_for(request.url().split('?').next().unwrap_or_default());

    let (status, result) = match file {
        None => (404, request.respond(Response::empty(404))),
        Some(file) if *request.method() == Method::Head => (
            200,
            request.respond(file_response(file, 200, 0, file.size, false)),
        ),
        Some(_) if *request.method() != Method::Get => (405, request.respond(Response::empty(405))),
        Some(file) => match parse_range(range_header.as_deref(), file.size) {
            Range::Whole => (
                200,
                request.respond(file_response(file, 200, 0, file.size, true)),
            ),
            Range::Part { start, end } => {
                let response =
                    file_response(file, 206, start, end + 1 - start, true).with_header(header(
                        "Content-Range",
                        &format!("bytes {start}-{end}/{}", file.size),
                    ));
                (206, request.respond(response))
            }
            Range::Unsatisfiable => {
                let response = Response::empty(416)
                    .with_header(header("Content-Range", &format!("bytes */{}", file.size)));
                (416, request.respond(response))
            }
        },
    };
    eprintln!(
        "  [http] {line} {status} {}",
        range_header.as_deref().unwrap_or("-")
    );
    if let Err(error) = result
        && !is_disconnect(&error)
    {
        eprintln!("  [http] error: {error}");
    }
}

fn file_response(
    file: &ServedFile,
    status: u16,
    start: u64,
    length: u64,
    body: bool,
) -> Response<Box<dyn Read + Send>> {
    let reader: Box<dyn Read + Send> = match (body, open_at(&file.path, start)) {
        (true, Ok(handle)) => Box::new(io::BufReader::with_capacity(CHUNK, handle.take(length))),
        _ => Box::new(io::empty()),
    };
    Response::new(
        StatusCode(status),
        vec![
            header("Content-Type", file.content_type),
            header("Accept-Ranges", "bytes"),
        ],
        reader,
        Some(length as usize),
        None,
    )
    // VLC needs Content-Length to know the size, and so to seek.
    .with_chunked_threshold(usize::MAX)
}

fn open_at(path: &Path, start: u64) -> io::Result<File> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    Ok(file)
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .expect("header names and values are ASCII")
}

/// VLC drops its connection on every seek, usually with a reset.
fn is_disconnect(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::TimedOut
    )
}

#[derive(Debug, PartialEq, Eq)]
enum Range {
    Whole,
    /// Inclusive at both ends.
    Part {
        start: u64,
        end: u64,
    },
    Unsatisfiable,
}

/// Which bytes a `Range` header asks for. Anything but a single range is
/// answered with the whole file, which is allowed.
fn parse_range(header: Option<&str>, size: u64) -> Range {
    let Some(spec) = header.and_then(|header| header.trim().strip_prefix("bytes=")) else {
        return Range::Whole;
    };
    let Some((first, last)) = spec.split_once('-') else {
        return Range::Whole;
    };
    let number = |text: &str| -> Option<Option<u64>> {
        if text.is_empty() {
            Some(None)
        } else if text.bytes().all(|byte| byte.is_ascii_digit()) {
            text.parse().ok().map(Some)
        } else {
            None
        }
    };
    let (Some(first), Some(last)) = (number(first), number(last)) else {
        return Range::Whole;
    };
    match (first, last) {
        (None, None) => Range::Whole,
        (None, Some(_)) if size == 0 => Range::Unsatisfiable,
        (None, Some(suffix)) => Range::Part {
            start: size.saturating_sub(suffix),
            end: size - 1,
        },
        (Some(start), _) if start >= size => Range::Unsatisfiable,
        (Some(start), last) => {
            let end = last.map_or(size - 1, |last| last.min(size - 1));
            if start > end {
                Range::Unsatisfiable
            } else {
                Range::Part { start, end }
            }
        }
    }
}

fn content_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "mkv" => "video/x-matroska",
        "mp4" => "video/mp4",
        "m4v" => "video/x-m4v",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "webm" => "video/webm",
        "ts" | "m2ts" => "video/mp2t",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mpg" | "mpeg" => "video/mpeg",
        "ogv" => "video/ogg",
        "3gp" => "video/3gpp",
        "srt" => "application/x-subrip",
        "vtt" => "text/vtt",
        "ass" | "ssa" => "text/x-ssa",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range(None, 100), Range::Whole);
        assert_eq!(
            parse_range(Some("bytes=0-"), 100),
            Range::Part { start: 0, end: 99 }
        );
        assert_eq!(
            parse_range(Some("bytes=10-19"), 100),
            Range::Part { start: 10, end: 19 }
        );
        assert_eq!(
            parse_range(Some("bytes=90-200"), 100),
            Range::Part { start: 90, end: 99 }
        );
        assert_eq!(
            parse_range(Some("bytes=-10"), 100),
            Range::Part { start: 90, end: 99 }
        );
        assert_eq!(
            parse_range(Some("bytes=-200"), 100),
            Range::Part { start: 0, end: 99 }
        );
        assert_eq!(parse_range(Some("bytes=100-"), 100), Range::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=20-10"), 100), Range::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=-"), 100), Range::Whole);
        assert_eq!(parse_range(Some("bytes=0-1,5-6"), 100), Range::Whole);
        assert_eq!(parse_range(Some("items=0-1"), 100), Range::Whole);
    }

    fn served(file_name: &str) -> ServedFile {
        ServedFile {
            path: PathBuf::new(),
            file_name: file_name.into(),
            size: 0,
            content_type: content_type(Path::new(file_name)),
        }
    }

    fn site() -> Site {
        Site {
            token: "abc".into(),
            files: vec![
                served("Fanny och Alexander (1982).mkv"),
                served("Fanny och Alexander (1982).sv.srt"),
            ],
        }
    }

    #[test]
    fn url_path_encodes_like_python() {
        let site = site();
        assert_eq!(
            site.url_path(&site.files[0]),
            "/abc/Fanny%20och%20Alexander%20%281982%29.mkv"
        );
    }

    #[test]
    fn requests_find_their_file() {
        let site = site();
        let name = |path: &str| site.file_for(path).map(|file| file.file_name.as_str());
        assert_eq!(
            name(&site.url_path(&site.files[1])),
            Some("Fanny och Alexander (1982).sv.srt")
        );
        assert_eq!(
            name("/abc/Fanny%20och%20Alexander%20(1982).sv.srt"),
            Some("Fanny och Alexander (1982).sv.srt")
        );
        assert_eq!(
            name(&site.url_path(&site.files[0])),
            Some("Fanny och Alexander (1982).mkv")
        );
        assert_eq!(name("/abc/other"), Some("Fanny och Alexander (1982).mkv"));
        assert_eq!(name("/xyz/Fanny%20och%20Alexander%20%281982%29.mkv"), None);
    }

    #[test]
    fn content_type_ignores_case() {
        assert_eq!(content_type(Path::new("a/B.MKV")), "video/x-matroska");
        assert_eq!(content_type(Path::new("a/b")), "application/octet-stream");
    }
}
