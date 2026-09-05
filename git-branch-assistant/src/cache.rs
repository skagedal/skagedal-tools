use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::services::git_repos_list_service::BranchListEntry;

const CACHE_TTL_SECS: i64 = 3600;

pub struct BranchCache {
    root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredCache {
    invocation_path: PathBuf,
    timestamp: i64,
    entries: Vec<BranchListEntry>,
}

impl BranchCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn from_env() -> Self {
        Self::new(skagedal_dirs::cache_dir("git-branch-assistant"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read_fresh(&self, path: &Path) -> Option<Vec<BranchListEntry>> {
        let cache_path = self.cache_file_for(path);
        let content = fs::read_to_string(&cache_path).ok()?;
        let cache: StoredCache = serde_json::from_str(&content).ok()?;
        if now_unix() - cache.timestamp > CACHE_TTL_SECS {
            return None;
        }
        Some(cache.entries)
    }

    pub fn write(&self, path: &Path, entries: &[BranchListEntry]) -> Result<()> {
        let cache_path = self.cache_file_for(path);
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create cache dir at {}", parent.display()))?;
        }
        let cache = StoredCache {
            invocation_path: path.to_path_buf(),
            timestamp: now_unix(),
            entries: entries.to_vec(),
        };
        let json = serde_json::to_string(&cache)?;
        fs::write(&cache_path, json)
            .with_context(|| format!("failed to write cache at {}", cache_path.display()))?;
        Ok(())
    }

    fn cache_file_for(&self, path: &Path) -> PathBuf {
        let key = format!("{:016x}", hash_path(path));
        self.root.join("branches").join(format!("{key}.json"))
    }
}

fn hash_path(path: &Path) -> u64 {
    fnv1a(path.to_string_lossy().as_bytes())
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::git_repos_list_service::BranchStatus;

    fn entry() -> BranchListEntry {
        BranchListEntry {
            repo_name: "repo".to_string(),
            repo_path: PathBuf::from("/tmp/repo"),
            refname: "main".to_string(),
            status: BranchStatus::Identical,
            commit_timestamp: 1000,
            commit_date: "2024-01-01".to_string(),
            committer: "alice".to_string(),
            worktree_path: None,
        }
    }

    #[test]
    fn write_then_read_round_trips_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let cache = BranchCache::new(temp.path().to_path_buf());
        let invocation = PathBuf::from("/tmp/some/projects");
        cache.write(&invocation, &[entry()])?;
        let read = cache.read_fresh(&invocation).expect("expected fresh cache");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].refname, "main");
        Ok(())
    }

    #[test]
    fn stale_cache_is_ignored() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let cache = BranchCache::new(temp.path().to_path_buf());
        let invocation = PathBuf::from("/tmp/another/projects");
        let cache_path = cache.cache_file_for(&invocation);
        fs::create_dir_all(cache_path.parent().unwrap())?;
        let stale = StoredCache {
            invocation_path: invocation.clone(),
            timestamp: now_unix() - CACHE_TTL_SECS - 60,
            entries: vec![entry()],
        };
        fs::write(&cache_path, serde_json::to_string(&stale)?)?;
        assert!(cache.read_fresh(&invocation).is_none());
        Ok(())
    }

    #[test]
    fn missing_cache_returns_none() {
        let temp = tempfile::tempdir().unwrap();
        let cache = BranchCache::new(temp.path().to_path_buf());
        assert!(
            cache
                .read_fresh(Path::new("/tmp/no/cache/here/yet"))
                .is_none()
        );
    }
}
