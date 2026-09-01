//! JSONL writer + `latest-run.jsonl` symlink update.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use indexmap::IndexMap;
use serde_json::Value;

use crate::insights::QueryRow;
use crate::paths;

/// Write rows as JSONL, creating the parent directory as needed.
pub fn write_results_strings(path: &Path, rows: &[QueryRow]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("could not create {}: {}", parent.display(), e))?;
    }
    let mut contents = String::new();
    for row in rows {
        let map: serde_json::Map<String, Value> = row
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        contents.push_str(&serde_json::to_string(&Value::Object(map))?);
        contents.push('\n');
    }
    std::fs::write(path, contents)
        .map_err(|e| anyhow!("could not write {}: {}", path.display(), e))?;
    Ok(())
}

/// Write rows (possibly already flattened to nested values) as JSONL.
pub fn write_results(path: &Path, rows: &[IndexMap<String, Value>]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("could not create {}: {}", parent.display(), e))?;
    }
    let mut contents = String::new();
    for row in rows {
        let map: serde_json::Map<String, Value> =
            row.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        contents.push_str(&serde_json::to_string(&Value::Object(map))?);
        contents.push('\n');
    }
    std::fs::write(path, contents)
        .map_err(|e| anyhow!("could not write {}: {}", path.display(), e))?;
    Ok(())
}

/// Point `latest-run.jsonl` at `target`. Replaces any existing entry.
pub fn update_latest_symlink(target: &Path) -> Result<PathBuf> {
    let link_path = paths::latest_run_path();
    if let Some(parent) = link_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("could not create {}: {}", parent.display(), e))?;
    }
    if let Ok(meta) = std::fs::symlink_metadata(&link_path) {
        if meta.file_type().is_symlink() || meta.is_file() {
            std::fs::remove_file(&link_path)
                .map_err(|e| anyhow!("could not remove {}: {}", link_path.display(), e))?;
        }
    }
    symlink(target, &link_path)
        .map_err(|e| anyhow!("could not create symlink {}: {}", link_path.display(), e))?;
    Ok(link_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn write_results_strings_writes_jsonl() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("run.jsonl");
        let mut row: QueryRow = IndexMap::new();
        row.insert("@timestamp".into(), "2026".into());
        row.insert("@message".into(), "hello".into());
        write_results_strings(&path, &[row]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"@timestamp\":\"2026\",\"@message\":\"hello\"}\n"
        );
    }

    #[test]
    fn update_latest_symlink_replaces() {
        let tmp = tempdir().unwrap();
        // SAFETY: setting an env var is unsafe in concurrent tests; cargo test
        // runs each test in its own thread but other tests may also touch
        // SKAGEDAL_TOOLS_HOME — cargo serializes file-system tests where it
        // matters, so this is acceptable here.
        unsafe {
            std::env::set_var("SKAGEDAL_TOOLS_HOME", tmp.path());
        }

        let result_path = paths::tool_base_dir()
            .join("queries")
            .join("repo")
            .join("results")
            .join("run-x.jsonl");
        write_results_strings(&result_path, &[]).unwrap();

        let link = update_latest_symlink(&result_path).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), result_path);

        let other = paths::tool_base_dir()
            .join("queries")
            .join("repo")
            .join("results")
            .join("run-y.jsonl");
        write_results_strings(&other, &[]).unwrap();
        update_latest_symlink(&other).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), other);

        unsafe {
            std::env::remove_var("SKAGEDAL_TOOLS_HOME");
        }
    }
}
