//! `disky.toml` parsing and pair resolution.
//!
//! The config declares *remotes* (ssh endpoints) and *pairs* (a local
//! directory plus where it lives on one of those remotes):
//!
//! ```toml
//! [remotes.hetzner]
//! host = "u656759.your-storagebox.de"
//! user = "u656759"
//! port = 23
//! identity_file = "~/.ssh/hetzner-key"
//!
//! [pairs.studio]
//! remote = "hetzner"
//! local = "~/studio"
//! path = "studio"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Remote {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Pair {
    /// Name of the remote in `[remotes.*]`. Omit for a pair that syncs to
    /// another path on this machine (an external disk, say).
    pub remote: Option<String>,
    pub local: String,
    pub path: String,
    /// Extra rsync flags for this pair only.
    #[serde(default)]
    pub rsync_extra: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub remotes: BTreeMap<String, Remote>,
    #[serde(default)]
    pub pairs: BTreeMap<String, Pair>,
    /// Extra rsync flags applied to every pair.
    #[serde(default)]
    pub rsync_extra: Vec<String>,
}

/// A pair with its name and remote resolved, and `local` expanded to an
/// absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPair {
    pub name: String,
    pub local: PathBuf,
    pub remote_path: String,
    pub remote: Option<Remote>,
    pub rsync_extra: Vec<String>,
}

impl ResolvedPair {
    /// The destination as rsync wants it: `user@host:path` for a remote pair,
    /// or a plain local path when no remote is configured.
    pub fn remote_spec(&self, project: &str) -> String {
        let base = match &self.remote {
            Some(r) => match &r.user {
                Some(u) => format!("{}@{}:{}", u, r.host, self.remote_path),
                None => format!("{}:{}", r.host, self.remote_path),
            },
            None => self.remote_path.clone(),
        };
        if project.is_empty() {
            format!("{base}/")
        } else {
            format!("{base}/{project}")
        }
    }

    /// How to describe the far side to a human.
    pub fn remote_label(&self) -> String {
        match &self.remote {
            Some(r) => format!("{}:{}", r.host, self.remote_path),
            None => self.remote_path.clone(),
        }
    }

    pub fn local_project(&self, project: &str) -> PathBuf {
        self.local.join(project)
    }
}

/// Path to the config file: `$DISKY_CONFIG`, else
/// `~/.config/skagedal-tools/disky/config.toml`.
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("DISKY_CONFIG") {
        return PathBuf::from(p);
    }
    skagedal_dirs::config_dir("disky").join("config.toml")
}

pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        bail!(
            "no config at {}. Create it with a [remotes.*] and a [pairs.*] block \
             — see the disky README.",
            path.display()
        );
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    parse(&text).with_context(|| format!("in {}", path.display()))
}

pub fn parse(text: &str) -> Result<Config> {
    toml::from_str(text).map_err(|e| anyhow!("failed to parse config: {e}"))
}

/// Expand a leading `~` and make the result absolute.
pub fn expand(p: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(p).into_owned())
}

impl Config {
    pub fn resolve(&self, name: &str) -> Result<ResolvedPair> {
        let pair = self.pairs.get(name).ok_or_else(|| {
            anyhow!(
                "no pair named {:?}. Known pairs: {}",
                name,
                self.pair_names()
            )
        })?;

        let remote = match &pair.remote {
            Some(rname) => Some(
                self.remotes
                    .get(rname)
                    .ok_or_else(|| {
                        anyhow!(
                            "pair {:?} refers to remote {:?}, which is not defined",
                            name,
                            rname
                        )
                    })?
                    .clone(),
            ),
            None => None,
        };

        let mut rsync_extra = self.rsync_extra.clone();
        rsync_extra.extend(pair.rsync_extra.iter().cloned());

        Ok(ResolvedPair {
            name: name.to_string(),
            local: expand(&pair.local),
            remote_path: pair.path.trim_end_matches('/').to_string(),
            remote,
            rsync_extra,
        })
    }

    pub fn pair_names(&self) -> String {
        if self.pairs.is_empty() {
            "(none configured)".to_string()
        } else {
            self.pairs.keys().cloned().collect::<Vec<_>>().join(", ")
        }
    }

    /// Which pair, if any, owns `dir` — i.e. `dir` is the pair's local root or
    /// lives underneath it. The longest matching root wins, so nested pairs
    /// resolve to the most specific one.
    ///
    /// Also returns the project name when `dir` is inside a project directory
    /// rather than at the root itself.
    pub fn pair_for_dir(&self, dir: &Path) -> Option<(ResolvedPair, Option<String>)> {
        let mut best: Option<(ResolvedPair, Option<String>)> = None;
        let mut best_len = 0usize;

        for name in self.pairs.keys() {
            let Ok(resolved) = self.resolve(name) else {
                continue;
            };
            // Compare canonicalised where possible so /tmp vs /private/tmp and
            // other symlinked roots still match.
            let root = canonical(&resolved.local);
            let here = canonical(dir);
            let Ok(rest) = here.strip_prefix(&root) else {
                continue;
            };
            let len = root.as_os_str().len();
            if len < best_len {
                continue;
            }
            let project = rest
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
            best_len = len;
            best = Some((resolved, project));
        }
        best
    }
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Reject names that could escape the pair root.
pub fn check_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("empty project name");
    }
    if name.contains('/') {
        bail!("project name must not contain '/': {name}");
    }
    if name == "." || name == ".." {
        bail!("bad project name: {name}");
    }
    if name.starts_with('-') {
        bail!("bad project name: {name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[remotes.hetzner]
host = "u656759.your-storagebox.de"
user = "u656759"
port = 23
identity_file = "~/.ssh/hetzner-key"

[pairs.studio]
remote = "hetzner"
local = "~/studio"
path = "studio"

[pairs.usb]
local = "~/studio"
path = "/Volumes/Backup/studio"
"#;

    #[test]
    fn parses_remotes_and_pairs() {
        let c = parse(SAMPLE).unwrap();
        assert_eq!(c.remotes.len(), 1);
        assert_eq!(c.pairs.len(), 2);
        let h = &c.remotes["hetzner"];
        assert_eq!(h.port, Some(23));
        assert_eq!(h.user.as_deref(), Some("u656759"));
    }

    #[test]
    fn remote_spec_includes_user_and_project() {
        let c = parse(SAMPLE).unwrap();
        let p = c.resolve("studio").unwrap();
        assert_eq!(
            p.remote_spec("about-a-girl"),
            "u656759@u656759.your-storagebox.de:studio/about-a-girl"
        );
        assert_eq!(
            p.remote_spec(""),
            "u656759@u656759.your-storagebox.de:studio/"
        );
    }

    #[test]
    fn pair_without_remote_is_a_plain_path() {
        let c = parse(SAMPLE).unwrap();
        let p = c.resolve("usb").unwrap();
        assert!(p.remote.is_none());
        assert_eq!(p.remote_spec("song"), "/Volumes/Backup/studio/song");
    }

    #[test]
    fn trailing_slash_on_path_is_not_doubled() {
        let c = parse(
            r#"
[pairs.p]
local = "/a"
path = "/b/"
"#,
        )
        .unwrap();
        assert_eq!(c.resolve("p").unwrap().remote_spec("x"), "/b/x");
    }

    #[test]
    fn unknown_pair_and_unknown_remote_are_errors() {
        let c = parse(SAMPLE).unwrap();
        assert!(c.resolve("nope").is_err());

        let c2 = parse(
            r#"
[pairs.p]
remote = "ghost"
local = "/a"
path = "b"
"#,
        )
        .unwrap();
        let err = c2.resolve("p").unwrap_err().to_string();
        assert!(err.contains("ghost"), "{err}");
    }

    #[test]
    fn rsync_extra_is_global_then_per_pair() {
        let c = parse(
            r#"
rsync_extra = ["--bwlimit=1000"]

[pairs.p]
local = "/a"
path = "/b"
rsync_extra = ["--xattrs"]
"#,
        )
        .unwrap();
        assert_eq!(
            c.resolve("p").unwrap().rsync_extra,
            vec!["--bwlimit=1000", "--xattrs"]
        );
    }

    #[test]
    fn finds_pair_from_directory_and_names_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("studio");
        std::fs::create_dir_all(root.join("about-a-girl/Audio Files")).unwrap();

        let c = parse(&format!(
            "[pairs.studio]\nlocal = {:?}\npath = \"studio\"\n",
            root.to_string_lossy()
        ))
        .unwrap();

        let (p, project) = c.pair_for_dir(&root).unwrap();
        assert_eq!(p.name, "studio");
        assert_eq!(project, None);

        let (_, project) = c.pair_for_dir(&root.join("about-a-girl")).unwrap();
        assert_eq!(project.as_deref(), Some("about-a-girl"));

        // Deeper inside a project still reports the top-level project name.
        let (_, project) = c
            .pair_for_dir(&root.join("about-a-girl/Audio Files"))
            .unwrap();
        assert_eq!(project.as_deref(), Some("about-a-girl"));

        assert!(c.pair_for_dir(tmp.path()).is_none());
    }

    #[test]
    fn rejects_names_that_escape_the_root() {
        assert!(check_project_name("about-a-girl").is_ok());
        assert!(check_project_name("about a girl").is_ok());
        for bad in ["", "..", ".", "../etc", "a/b", "--delete"] {
            assert!(check_project_name(bad).is_err(), "should reject {bad:?}");
        }
    }
}
