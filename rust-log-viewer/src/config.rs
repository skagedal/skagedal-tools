use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

pub const DEFAULT_FIELD: &str = "message";

/// User-facing configuration parsed from TOML. Profiles are resolved into
/// [`Config`] before reaching the rest of the app.
#[derive(Debug, Clone, Default, Deserialize)]
struct RawConfig {
    #[serde(default)]
    default_field: Option<String>,
    #[serde(default)]
    fields: Vec<FieldDef>,
    #[serde(default)]
    profiles: Vec<RawProfile>,
    #[serde(default)]
    triggers: Vec<RawTrigger>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawProfile {
    name: String,
    #[serde(default)]
    default_field: Option<String>,
    #[serde(default)]
    fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(default)]
    pub from: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTrigger {
    pub name: String,
    pub on_new_value: StringOrList,
    pub action: String,
    #[serde(default)]
    pub startup_delay_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum StringOrList {
    One(String),
    Many(Vec<String>),
}

impl StringOrList {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s],
            Self::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub default_field: String,
    pub fields: Vec<FieldDef>,
    pub triggers: Vec<TriggerDef>,
}

#[derive(Debug, Clone)]
pub struct TriggerDef {
    pub name: String,
    pub on_new_value: Vec<String>,
    pub action: String,
    pub startup_delay_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_field: DEFAULT_FIELD.to_string(),
            fields: default_fields(),
            triggers: Vec::new(),
        }
    }
}

fn default_fields() -> Vec<FieldDef> {
    vec![
        FieldDef {
            name: "time".into(),
            from: vec_of(&["@timestamp", "timestamp", "time", "ts"]),
        },
        FieldDef {
            name: "level".into(),
            from: vec_of(&["level", "severity", "lvl"]),
        },
        FieldDef {
            name: "message".into(),
            from: vec_of(&["message", "msg", "@message"]),
        },
    ]
}

fn vec_of(slice: &[&str]) -> Vec<String> {
    slice.iter().map(|s| (*s).to_string()).collect()
}

fn finalize(raw: RawConfig) -> Config {
    let default_field = raw
        .default_field
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_FIELD.to_string());
    let fields = if raw.fields.is_empty() {
        default_fields()
    } else {
        raw.fields
    };
    let triggers = raw
        .triggers
        .into_iter()
        .map(|t| TriggerDef {
            name: t.name,
            on_new_value: t.on_new_value.into_vec(),
            action: t.action,
            startup_delay_ms: t.startup_delay_ms,
        })
        .collect();
    Config {
        default_field,
        fields,
        triggers,
    }
}

pub fn load_with_profile(path: &Path, profile: Option<&str>) -> Result<Config> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if let Some(name) = profile {
                return Err(anyhow!(
                    "profile '{name}' requested but config file does not exist at {}",
                    path.display()
                ));
            }
            return Ok(Config::default());
        }
        Err(err) => {
            return Err(anyhow::Error::new(err)
                .context(format!("reading config at {}", path.display())));
        }
    };
    let raw: RawConfig = toml::from_str(&text)
        .with_context(|| format!("parsing config at {}", path.display()))?;
    if let Some(name) = profile {
        let profile = raw
            .profiles
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow!("unknown profile: {name}"))?
            .clone();
        let merged = RawConfig {
            default_field: profile.default_field.or(raw.default_field),
            fields: if profile.fields.is_empty() {
                raw.fields
            } else {
                profile.fields
            },
            profiles: Vec::new(),
            triggers: raw.triggers,
        };
        return Ok(finalize(merged));
    }
    Ok(finalize(raw))
}

pub fn config_path() -> PathBuf {
    if let Ok(custom) = std::env::var("RUST_LOG_VIEWER_CONFIG") {
        return PathBuf::from(custom);
    }
    base_dir().join("config.toml")
}

fn base_dir() -> PathBuf {
    let home = std::env::var("SKAGEDAL_TOOLS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("."));
            PathBuf::from(home).join(".skagedal-tools")
        });
    home.join("rust-log-viewer")
}

/// Create a starter config at `path` if it doesn't already exist. Returns
/// `true` if a file was created.
pub fn ensure_config_file(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, STARTER_CONFIG)
        .with_context(|| format!("writing starter config to {}", path.display()))?;
    Ok(true)
}

const STARTER_CONFIG: &str = r#"# rust-log-viewer config (TOML)

# Field name used to wrap lines that aren't valid JSON.
default_field = "message"

[[fields]]
name = "time"
from = ["@timestamp", "timestamp", "time", "ts"]

[[fields]]
name = "level"
from = ["level", "severity", "lvl"]

[[fields]]
name = "message"
from = ["message", "msg", "@message"]

# Profiles override `fields` (and optionally `default_field`) when picked
# at startup with `--profile <name>`.
#
# [[profiles]]
# name = "stern"
#
# [[profiles.fields]]
# name = "time"
# from = ["timestamp"]
#
# [[profiles.fields]]
# name = "pod"
# from = ["podName"]

# Triggers run a shell command the first time `on_new_value` takes a
# new value. {value} and {field} are substituted as shell-quoted strings.
#
# [[triggers]]
# name = "pod-deployed"
# on_new_value = "podname"
# action = "say new pod {value} deployed"
# startup_delay_ms = 2000
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_falls_back_to_defaults() {
        let cfg = finalize(RawConfig::default());
        assert_eq!(cfg.default_field, DEFAULT_FIELD);
        assert_eq!(cfg.fields.len(), 3);
        assert!(cfg.triggers.is_empty());
    }

    #[test]
    fn parses_full_toml() {
        let raw: RawConfig = toml::from_str(
            r#"
default_field = "msg"

[[fields]]
name = "time"
from = ["timestamp"]

[[triggers]]
name = "t1"
on_new_value = "pod"
action = "echo {value}"
startup_delay_ms = 500

[[triggers]]
name = "t2"
on_new_value = ["a", "b"]
action = "echo {field}={value}"
"#,
        )
        .unwrap();
        let cfg = finalize(raw);
        assert_eq!(cfg.default_field, "msg");
        assert_eq!(cfg.fields.len(), 1);
        assert_eq!(cfg.fields[0].name, "time");
        assert_eq!(cfg.triggers.len(), 2);
        assert_eq!(cfg.triggers[0].on_new_value, vec!["pod"]);
        assert_eq!(cfg.triggers[1].on_new_value, vec!["a", "b"]);
        assert_eq!(cfg.triggers[0].startup_delay_ms, 500);
        assert_eq!(cfg.triggers[1].startup_delay_ms, 0);
    }

    #[test]
    fn applies_profile_override() {
        let dir = tempdir();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            r#"
default_field = "message"

[[fields]]
name = "time"
from = ["@timestamp"]

[[profiles]]
name = "stern"
default_field = "msg"

[[profiles.fields]]
name = "pod"
from = ["podName"]
"#,
        )
        .unwrap();
        let cfg = load_with_profile(&path, Some("stern")).unwrap();
        assert_eq!(cfg.default_field, "msg");
        assert_eq!(cfg.fields.len(), 1);
        assert_eq!(cfg.fields[0].name, "pod");
    }

    #[test]
    fn unknown_profile_errors() {
        let dir = tempdir();
        let path = dir.join("config.toml");
        fs::write(&path, "default_field = \"message\"\n").unwrap();
        let err = load_with_profile(&path, Some("nope")).unwrap_err();
        assert!(err.to_string().contains("unknown profile"));
    }

    #[test]
    fn missing_file_with_no_profile_returns_defaults() {
        let cfg = load_with_profile(Path::new("/no/such/file.toml"), None).unwrap();
        assert_eq!(cfg.default_field, DEFAULT_FIELD);
    }

    #[test]
    fn missing_file_with_profile_errors() {
        let err =
            load_with_profile(Path::new("/no/such/file.toml"), Some("stern")).unwrap_err();
        assert!(err.to_string().contains("profile 'stern'"));
    }

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut p = std::env::temp_dir();
        p.push(format!("rlv-test-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
}
