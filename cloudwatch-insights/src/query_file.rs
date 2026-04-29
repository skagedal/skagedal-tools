//! Per-repo `.insights` query files: TOML front-matter + query body.

use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::default_query;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct QueryFileError(pub String);

/// Front-matter (de)serialization. `log-group` may be a string or array.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FrontMatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(
        default,
        rename = "log-group",
        skip_serializing_if = "Option::is_none"
    )]
    pub log_group: Option<LogGroupValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LogGroupValue {
    Single(String),
    Multiple(Vec<String>),
}

impl LogGroupValue {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            LogGroupValue::Single(s) => vec![s.clone()],
            LogGroupValue::Multiple(v) => v.clone(),
        }
    }
}

#[derive(Debug)]
pub struct QueryFile {
    pub front_matter: FrontMatter,
    pub body: String,
}

/// Parse a `.insights` file: optional TOML front-matter followed by a single
/// `---` separator line and then the query body. With no separator, the whole
/// file is treated as the body.
pub fn parse_query_file(contents: &str) -> Result<QueryFile, QueryFileError> {
    let stripped = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let lines: Vec<&str> = stripped.lines().collect();
    let sep_idx = lines.iter().position(|l| l.trim() == "---");

    match sep_idx {
        None => Ok(QueryFile {
            front_matter: FrontMatter::default(),
            body: stripped.trim().to_string(),
        }),
        Some(idx) => {
            let toml_text = lines[..idx].join("\n");
            let body = lines[idx + 1..].join("\n").trim().to_string();
            let fm: FrontMatter = if toml_text.trim().is_empty() {
                FrontMatter::default()
            } else {
                toml::from_str(&toml_text)
                    .map_err(|e| QueryFileError(format!("front-matter parse error: {e}")))?
            };
            Ok(QueryFile {
                front_matter: fm,
                body,
            })
        }
    }
}

pub fn load_query_file(path: &Path) -> Result<QueryFile, QueryFileError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| QueryFileError(format!("could not read {}: {}", path.display(), e)))?;
    parse_query_file(&raw)
}

#[derive(Debug, Default, Clone)]
pub struct SeedFrontMatter {
    pub time: Option<String>,
    pub env: Option<String>,
    pub app: Option<String>,
    pub log_group: Option<LogGroupValue>,
}

#[derive(Debug, Default)]
pub struct SeedOptions<'a> {
    pub app: Option<&'a str>,
    pub front_matter: SeedFrontMatter,
}

/// Ensure `path` exists, seeding it on absence. Returns whether a new file
/// was written.
pub fn ensure_current_insights(path: &Path, options: &SeedOptions<'_>) -> Result<bool, QueryFileError> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| QueryFileError(format!("could not create {}: {}", parent.display(), e)))?;
    }
    std::fs::write(path, seed_content(options))
        .map_err(|e| QueryFileError(format!("could not write {}: {}", path.display(), e)))?;
    Ok(true)
}

/// Unconditionally reset `path` to the default template (used by `query --new`).
pub fn reset_current_insights(path: &Path, options: &SeedOptions<'_>) -> Result<(), QueryFileError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| QueryFileError(format!("could not create {}: {}", parent.display(), e)))?;
    }
    std::fs::write(path, seed_content(options))
        .map_err(|e| QueryFileError(format!("could not write {}: {}", path.display(), e)))?;
    Ok(())
}

fn seed_content(options: &SeedOptions<'_>) -> String {
    let fm = render_front_matter(&options.front_matter);
    let body = default_query(options.app);
    format!("{fm}---\n{body}\n")
}

fn render_front_matter(fm: &SeedFrontMatter) -> String {
    let mut out = String::new();
    if let Some(t) = &fm.time {
        out.push_str(&format!("time = {}\n", toml_string(t)));
    }
    if let Some(e) = &fm.env {
        out.push_str(&format!("env = {}\n", toml_string(e)));
    }
    if let Some(a) = &fm.app {
        out.push_str(&format!("app = {}\n", toml_string(a)));
    }
    if let Some(lg) = &fm.log_group {
        match lg {
            LogGroupValue::Single(s) => {
                out.push_str(&format!("log-group = {}\n", toml_string(s)));
            }
            LogGroupValue::Multiple(v) => {
                let parts: Vec<String> = v.iter().map(|s| toml_string(s)).collect();
                out.push_str(&format!("log-group = [{}]\n", parts.join(", ")));
            }
        }
    }
    out
}

fn toml_string(s: &str) -> String {
    let value = toml::Value::String(s.to_string());
    toml::to_string(&value).unwrap_or_else(|_| format!("\"{s}\""))
        .trim()
        .to_string()
}

/// Build a filesystem-safe ISO-ish timestamp (no colons): `2026-04-23T14-30-45Z`.
pub fn run_timestamp(now: DateTime<Utc>) -> String {
    let s = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    s.replace(':', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    #[test]
    fn no_front_matter_means_whole_body() {
        let qf = parse_query_file("fields @timestamp, @message | sort @timestamp desc").unwrap();
        assert!(qf.front_matter.time.is_none());
        assert_eq!(qf.body, "fields @timestamp, @message | sort @timestamp desc");
    }

    #[test]
    fn parses_toml_front_matter() {
        let qf = parse_query_file(
            "time = \"5h\"\nlog-group = \"/prod/team\"\n---\nfields @timestamp, @message\n| sort @timestamp desc\n",
        )
        .unwrap();
        assert_eq!(qf.front_matter.time.as_deref(), Some("5h"));
        match qf.front_matter.log_group {
            Some(LogGroupValue::Single(ref s)) => assert_eq!(s, "/prod/team"),
            _ => panic!("expected single log-group"),
        }
        assert_eq!(qf.body, "fields @timestamp, @message\n| sort @timestamp desc");
    }

    #[test]
    fn front_matter_log_group_can_be_array() {
        let qf =
            parse_query_file("log-group = [\"/a\", \"/b\"]\n---\nfields @timestamp").unwrap();
        match qf.front_matter.log_group {
            Some(LogGroupValue::Multiple(v)) => assert_eq!(v, vec!["/a".to_string(), "/b".to_string()]),
            _ => panic!("expected multiple log-groups"),
        }
    }

    #[test]
    fn missing_separator_treated_as_body() {
        let raw = "time = \"5h\"\nfields @timestamp, @message";
        let qf = parse_query_file(raw).unwrap();
        assert!(qf.front_matter.time.is_none());
        assert_eq!(qf.body, raw);
    }

    #[test]
    fn empty_front_matter() {
        let qf = parse_query_file("---\nfields @timestamp").unwrap();
        assert!(qf.front_matter.time.is_none());
        assert_eq!(qf.body, "fields @timestamp");
    }

    #[test]
    fn ensure_current_insights_seeds_default() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("current.insights");
        let created = ensure_current_insights(
            &path,
            &SeedOptions {
                app: Some("my-service"),
                front_matter: SeedFrontMatter::default(),
            },
        )
        .unwrap();
        assert!(created);
        let qf = load_query_file(&path).unwrap();
        assert!(qf.body.contains("filter app = '{{ app }}'"));
        assert!(!qf.body.contains("filter app = 'my-service'"));
        assert!(qf.body.contains("filter level in ['WARN', 'ERROR']"));
        assert!(qf.body.contains("limit 200"));
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("---\n"));
    }

    #[test]
    fn ensure_current_insights_omits_app_filter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("current.insights");
        ensure_current_insights(&path, &SeedOptions::default()).unwrap();
        let qf = load_query_file(&path).unwrap();
        assert!(!qf.body.contains("filter app"));
        assert!(qf.body.contains("filter level in ['WARN', 'ERROR']"));
    }

    #[test]
    fn ensure_current_insights_prefills_front_matter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("current.insights");
        ensure_current_insights(
            &path,
            &SeedOptions {
                app: Some("svc"),
                front_matter: SeedFrontMatter {
                    time: Some("5h".into()),
                    env: Some("prod".into()),
                    app: Some("svc".into()),
                    log_group: Some(LogGroupValue::Single("/{{ env }}/logs".into())),
                },
            },
        )
        .unwrap();
        let qf = load_query_file(&path).unwrap();
        assert_eq!(qf.front_matter.time.as_deref(), Some("5h"));
        assert_eq!(qf.front_matter.env.as_deref(), Some("prod"));
        assert_eq!(qf.front_matter.app.as_deref(), Some("svc"));
        match qf.front_matter.log_group {
            Some(LogGroupValue::Single(s)) => assert_eq!(s, "/{{ env }}/logs"),
            _ => panic!("expected single log-group"),
        }
    }

    #[test]
    fn ensure_current_insights_array_log_group() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("current.insights");
        ensure_current_insights(
            &path,
            &SeedOptions {
                app: None,
                front_matter: SeedFrontMatter {
                    log_group: Some(LogGroupValue::Multiple(vec!["/a".into(), "/b".into()])),
                    ..SeedFrontMatter::default()
                },
            },
        )
        .unwrap();
        let qf = load_query_file(&path).unwrap();
        match qf.front_matter.log_group {
            Some(LogGroupValue::Multiple(v)) => assert_eq!(v, vec!["/a".to_string(), "/b".to_string()]),
            _ => panic!("expected multiple log-groups"),
        }
    }

    #[test]
    fn ensure_current_insights_idempotent() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("current.insights");
        std::fs::write(&path, "time = \"1h\"\n---\nfields @timestamp\n").unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let created = ensure_current_insights(
            &path,
            &SeedOptions {
                app: Some("other-app"),
                front_matter: SeedFrontMatter::default(),
            },
        )
        .unwrap();
        assert!(!created);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn reset_current_insights_overwrites() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("current.insights");
        std::fs::write(&path, "---\nuser-modified query").unwrap();
        reset_current_insights(
            &path,
            &SeedOptions {
                app: Some("my-service"),
                front_matter: SeedFrontMatter::default(),
            },
        )
        .unwrap();
        let qf = load_query_file(&path).unwrap();
        assert!(!qf.body.contains("user-modified"));
        assert!(qf.body.contains("filter app = '{{ app }}'"));
    }

    #[test]
    fn run_timestamp_filesystem_safe() {
        let now = Utc.with_ymd_and_hms(2026, 4, 23, 14, 30, 45).unwrap();
        assert_eq!(run_timestamp(now), "2026-04-23T14-30-45Z");
    }
}
