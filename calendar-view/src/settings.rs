//! `~/.config/skagedal-tools/calendar-view/settings.toml`: which calendula
//! accounts to read, how to label and colour their calendars, and which are
//! shown before anything is toggled.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

const TOOL: &str = "calendar-view";

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// The calendula binary.
    pub calendula: Option<String>,
    /// Passed to calendula as `-c`, each in turn.
    pub config: Vec<String>,
    /// Files whose change means the events may have changed, typically the
    /// pimdir store a sync engine fills. Events are cached until one does.
    pub watch: Vec<String>,
    /// Run while the view is open, at start and every `sync-minutes`:
    /// `["neverest", "sync"]`, say, to keep the store `watch` names current.
    pub sync: Vec<String>,
    #[serde(rename = "sync-minutes")]
    pub sync_minutes: Option<u64>,
    /// By calendula account name. With none, every account calendula knows.
    pub accounts: BTreeMap<String, Account>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Account {
    /// Addresses that are me: an event where one of them declined is left out.
    pub me: Vec<String>,
    /// Calendars shown at start, by name. With none, every calendar is.
    pub shown: Vec<String>,
    /// Per calendar name.
    pub calendars: BTreeMap<String, Calendar>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Calendar {
    pub label: Option<String>,
    pub color: Option<String>,
}

/// `$CALENDAR_VIEW_CONFIG`, else `settings.toml` in the tool's config dir.
pub fn path() -> PathBuf {
    if let Ok(path) = std::env::var("CALENDAR_VIEW_CONFIG") {
        return PathBuf::from(path);
    }
    skagedal_dirs::config_dir(TOOL).join("settings.toml")
}

/// The settings, or the defaults when the file does not exist.
pub fn load(path: &Path) -> Result<Settings> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("reading {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

pub fn expand(path: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(path).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_account() {
        let settings: Settings = toml::from_str(
            r##"
            watch = ["~/.local/state/neverest/pimdir.db"]
            sync = ["neverest", "sync"]
            sync-minutes = 10

            [accounts.work]
            me = ["me@example.com"]
            shown = ["Calendar"]
            calendars.Calendar = { label = "Work", color = "#d03b3b" }
            "##,
        )
        .unwrap();
        assert_eq!(settings.sync_minutes, Some(10));
        let work = &settings.accounts["work"];
        assert_eq!(work.shown, ["Calendar"]);
        assert_eq!(work.calendars["Calendar"].label.as_deref(), Some("Work"));
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = load(&dir.path().join("settings.toml")).unwrap();
        assert!(settings.accounts.is_empty());
    }
}
