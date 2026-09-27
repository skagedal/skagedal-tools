//! Comments written against single transactions in `kontoutdrag view`.
//!
//! They are notes for whoever turns them into rules later — what a payment
//! was for, who a number belongs to — so each one carries enough of its
//! transaction to write a mark from without opening the statement again.
//! The file is plain JSON, one object per comment, sorted by date.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

/// The transaction a comment is about, as the view identifies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    /// The view's key for the transaction; see `commands::view::key`.
    pub key: String,
    pub account: String,
    /// Booking date, YYYY-MM-DD.
    pub date: String,
    /// Signed, as in the statement: negative is money out.
    pub amount: String,
    /// The descriptor key the tables are matched against.
    pub descriptor: String,
    /// The free-text field as the statement has it.
    pub text: String,
    /// The bank's own identifier, when the statement carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    #[serde(flatten)]
    pub subject: Subject,
    pub comment: String,
    /// When the comment was last changed, RFC 3339.
    pub updated: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default)]
    comments: Vec<Comment>,
}

/// Every comment in the file. A missing or empty file is no comments, so
/// clearing it by deleting it or by emptying it both work.
pub fn load(path: &Path) -> Result<Vec<Comment>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let file: File = serde_json::from_str(&text)
        .with_context(|| format!("could not parse {}", path.display()))?;
    if file.version != VERSION {
        bail!(
            "{}: comments file version {} is not supported (this build reads version {VERSION})",
            path.display(),
            file.version
        );
    }
    Ok(file.comments)
}

/// Set the comment on one transaction, or remove it when `text` is blank.
pub fn upsert(path: &Path, subject: Subject, text: &str, now: &str) -> Result<()> {
    let mut comments = load(path)?;
    comments.retain(|c| c.subject.key != subject.key);
    if !text.trim().is_empty() {
        comments.push(Comment {
            subject,
            comment: text.to_string(),
            updated: now.to_string(),
        });
    }
    comments
        .sort_by(|a, b| (&a.subject.date, &a.subject.key).cmp(&(&b.subject.date, &b.subject.key)));
    save(path, comments)
}

fn save(path: &Path, comments: Vec<Comment>) -> Result<()> {
    let body = serde_json::to_string_pretty(&File {
        version: VERSION,
        comments,
    })?;
    let path = &through_links(path)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create {}", dir.display()))?;
    }
    // Written beside the file and renamed over it, so a reader never sees
    // half of it.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body + "\n")
        .with_context(|| format!("could not write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("could not replace {}", path.display()))
}

/// The file a symlink finally points at, so that writing replaces the file
/// and leaves the link in place. Renaming onto the link itself would turn it
/// into an ordinary file.
fn through_links(path: &Path) -> Result<std::path::PathBuf> {
    let mut path = path.to_path_buf();
    for _ in 0..40 {
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = std::fs::read_link(&path)
                    .with_context(|| format!("could not read the link {}", path.display()))?;
                path = match path.parent() {
                    Some(dir) if target.is_relative() => dir.join(target),
                    _ => target,
                };
            }
            _ => return Ok(path),
        }
    }
    bail!("{}: too many levels of symbolic links", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(key: &str, date: &str) -> Subject {
        Subject {
            key: key.to_string(),
            account: "everyday".to_string(),
            date: date.to_string(),
            amount: "-200.00".to_string(),
            descriptor: "46700000001".to_string(),
            text: "46700000001".to_string(),
            reference: None,
        }
    }

    #[test]
    fn a_missing_or_empty_file_is_no_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comments.json");
        assert!(load(&path).unwrap().is_empty());
        std::fs::write(&path, "\n").unwrap();
        assert!(load(&path).unwrap().is_empty());
    }

    #[test]
    fn upsert_adds_replaces_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comments.json");
        upsert(&path, subject("b", "2026-02-01"), "groceries", "t1").unwrap();
        upsert(&path, subject("a", "2026-01-01"), "a jacket", "t2").unwrap();
        upsert(
            &path,
            subject("b", "2026-02-01"),
            "groceries for the house",
            "t3",
        )
        .unwrap();

        let all = load(&path).unwrap();
        assert_eq!(all.len(), 2);
        // Sorted by date, and the second write to `b` replaced the first.
        assert_eq!(all[0].subject.key, "a");
        assert_eq!(all[1].comment, "groceries for the house");
        assert_eq!(all[1].updated, "t3");

        upsert(&path, subject("a", "2026-01-01"), "  ", "t4").unwrap();
        assert_eq!(load(&path).unwrap().len(), 1);
    }

    /// A comments file kept elsewhere through a symlink stays linked.
    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("repo").join("comments.json");
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, "").unwrap();
        let link = dir.path().join("data").join("comments.json");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink("../repo/comments.json", &link).unwrap();

        upsert(&link, subject("a", "2026-01-01"), "a jacket", "t").unwrap();

        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(load(&real).unwrap()[0].comment, "a jacket");
    }

    #[test]
    fn creates_the_directory_on_first_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-yet").join("comments.json");
        upsert(&path, subject("a", "2026-01-01"), "a jacket", "t").unwrap();
        assert_eq!(load(&path).unwrap().len(), 1);
    }

    #[test]
    fn the_subject_is_written_flat_beside_the_comment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comments.json");
        upsert(&path, subject("a", "2026-01-01"), "a jacket", "t").unwrap();
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let first = &raw["comments"][0];
        assert_eq!(first["descriptor"], "46700000001");
        assert_eq!(first["comment"], "a jacket");
        assert!(first.get("reference").is_none());
    }
}
