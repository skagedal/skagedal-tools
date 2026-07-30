//! URL-to-note-path normalization, and a thin wrapper around the `obsidian`
//! CLI for looking up, creating, and opening notes.

use anyhow::{Result, bail};
use percent_encoding::percent_decode_str;

use crate::cli::run;

pub struct Note {
    pub path: String,
    pub exists: bool,
    pub content: Option<String>,
}

/// Computes `<folder>/<domain>/<normalized-path>.md` for a URL. Only
/// `https://` is supported (per spec, other schemes return `None`). Path
/// segments are percent-decoded for readability and joined with `__`,
/// since a literal `/` can't be part of a single file name; an empty path
/// (e.g. `https://example.com`) becomes `index`.
pub fn note_path(url_str: &str, folder: &str) -> Option<String> {
    let parsed = url::Url::parse(url_str).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let domain = parsed.host_str()?.to_lowercase();

    let segments: Vec<String> = parsed
        .path_segments()
        .into_iter()
        .flatten()
        .map(|segment| percent_decode_str(segment).decode_utf8_lossy().into_owned())
        .filter(|segment| !segment.is_empty())
        .map(sanitize_segment)
        .collect();

    let normalized = if segments.is_empty() {
        "index".to_string()
    } else {
        segments.join("__")
    };

    Some(format!("{folder}/{domain}/{normalized}.md"))
}

/// Neutralizes characters that would otherwise corrupt the `__`-joined file
/// name, e.g. a percent-encoded `/` decoded back into a literal slash.
fn sanitize_segment(segment: String) -> String {
    segment
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .collect()
}

pub fn initial_content(url: &str) -> String {
    let timestamp = chrono::Utc::now().to_rfc3339();
    format!("---\nurl: {url}\ncreated: {timestamp}\n---\n\n")
}

pub fn lookup(binary: &str, vault: &str, path: &str) -> Result<Note> {
    let info = run(binary, &["file".to_string(), format!("path={path}"), format!("vault={vault}")])?;
    if is_error(&info) {
        return Ok(Note { path: path.to_string(), exists: false, content: None });
    }
    let content = run(binary, &["read".to_string(), format!("path={path}"), format!("vault={vault}")])?;
    Ok(Note { path: path.to_string(), exists: true, content: Some(content) })
}

/// Idempotent: if the note already exists, returns it as-is rather than
/// calling `obsidian create` again (which would create a numbered
/// duplicate like `foo 1.md` instead of erroring or overwriting).
pub fn create(binary: &str, vault: &str, path: &str, content: &str) -> Result<Note> {
    let existing = lookup(binary, vault, path)?;
    if existing.exists {
        return Ok(existing);
    }

    let out = run(
        binary,
        &[
            "create".to_string(),
            format!("path={path}"),
            format!("content={content}"),
            format!("vault={vault}"),
        ],
    )?;
    if is_error(&out) {
        bail!("obsidian create failed: {}", out.trim());
    }
    Ok(Note { path: path.to_string(), exists: true, content: Some(content.to_string()) })
}

pub fn open(binary: &str, vault: &str, path: &str) -> Result<()> {
    let out = run(binary, &["open".to_string(), format!("path={path}"), format!("vault={vault}")])?;
    if is_error(&out) {
        bail!("obsidian open failed: {}", out.trim());
    }
    Ok(())
}

fn is_error(cli_stdout: &str) -> bool {
    cli_stdout.trim_start().starts_with("Error:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_path_segments_with_double_underscore() {
        assert_eq!(
            note_path("https://github.com/foo/bar", "webnotes"),
            Some("webnotes/github.com/foo__bar.md".to_string())
        );
    }

    #[test]
    fn root_path_becomes_index() {
        assert_eq!(note_path("https://example.com", "webnotes"), Some("webnotes/example.com/index.md".to_string()));
        assert_eq!(note_path("https://example.com/", "webnotes"), Some("webnotes/example.com/index.md".to_string()));
    }

    #[test]
    fn trailing_slash_is_ignored() {
        assert_eq!(
            note_path("https://github.com/foo/bar/", "webnotes"),
            Some("webnotes/github.com/foo__bar.md".to_string())
        );
    }

    #[test]
    fn domain_is_lowercased() {
        assert_eq!(
            note_path("https://GitHub.com/foo", "webnotes"),
            Some("webnotes/github.com/foo.md".to_string())
        );
    }

    #[test]
    fn percent_encoded_segments_are_decoded() {
        assert_eq!(
            note_path("https://example.com/hello%20world", "webnotes"),
            Some("webnotes/example.com/hello world.md".to_string())
        );
    }

    #[test]
    fn percent_encoded_slash_within_a_segment_is_sanitized() {
        assert_eq!(
            note_path("https://example.com/foo%2Fbar", "webnotes"),
            Some("webnotes/example.com/foo_bar.md".to_string())
        );
    }

    #[test]
    fn query_and_fragment_are_ignored() {
        assert_eq!(
            note_path("https://example.com/search?q=hello#frag", "webnotes"),
            Some("webnotes/example.com/search.md".to_string())
        );
    }

    #[test]
    fn non_https_schemes_are_unsupported() {
        assert_eq!(note_path("http://example.com/foo", "webnotes"), None);
        assert_eq!(note_path("chrome://newtab/", "webnotes"), None);
    }
}
