//! `settings.toml` parsing, merging, and seeding.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::git;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

/// AWS-related settings scoped to a single environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvConfig {
    pub aws_profile: Option<String>,
    pub region: Option<String>,
}

/// Configuration that applies to a repository — either as the top-level
/// `[defaults]` block or a per-repo `[repo.<name>]` block. Per-repo values
/// fully replace defaults values (arrays are not merged).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoConfig {
    pub group: Option<String>,
    pub app: Option<String>,
    pub flatten_fields: Option<Vec<String>>,
    pub region: Option<String>,
    pub env_overrides: Option<BTreeMap<String, EnvConfig>>,
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub defaults: RepoConfig,
    pub repos: BTreeMap<String, RepoConfig>,
    pub environments: BTreeMap<String, EnvConfig>,
}

/// Returns true if `name` is a syntactically valid environment name —
/// lower kebab-case: starts with a letter, segments separated by single
/// dashes, no underscores or uppercase.
pub fn is_valid_environment_name(name: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$").unwrap())
        .is_match(name)
}

#[derive(Debug, Deserialize)]
struct RawDoc {
    #[serde(default)]
    defaults: Option<toml::Table>,
    #[serde(default)]
    repo: Option<toml::Table>,
    #[serde(default)]
    env: Option<toml::Table>,
}

/// Load settings from disk. A missing file yields empty settings.
pub fn load_settings(path: &Path) -> Result<Settings, ConfigError> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ConfigError(format!("could not read {}: {}", path.display(), e)))?;
    parse_settings(&contents)
}

/// Parse a TOML string into structured settings. Exported for tests.
pub fn parse_settings(toml_str: &str) -> Result<Settings, ConfigError> {
    let raw: RawDoc = toml::from_str(toml_str)
        .map_err(|e| ConfigError(format!("failed to parse settings.toml: {e}")))?;

    let defaults = raw
        .defaults
        .as_ref()
        .map(parse_repo_config)
        .unwrap_or_default();

    let mut repos: BTreeMap<String, RepoConfig> = BTreeMap::new();
    if let Some(repo_table) = raw.repo.as_ref() {
        for (name, value) in repo_table {
            if let toml::Value::Table(t) = value {
                repos.insert(name.clone(), parse_repo_config(t));
            }
        }
    }

    let environments = raw
        .env
        .as_ref()
        .map(parse_environment_map)
        .unwrap_or_default();

    Ok(Settings {
        defaults,
        repos,
        environments,
    })
}

fn parse_repo_config(section: &toml::Table) -> RepoConfig {
    let mut config = RepoConfig::default();
    if let Some(toml::Value::String(s)) = section.get("group") {
        config.group = Some(s.clone());
    }
    if let Some(toml::Value::String(s)) = section.get("app") {
        config.app = Some(s.clone());
    }
    if let Some(toml::Value::String(s)) = section.get("region") {
        config.region = Some(s.clone());
    }
    if let Some(toml::Value::Array(arr)) = section.get("flatten-fields") {
        let fields: Vec<String> = arr
            .iter()
            .filter_map(|v| match v {
                toml::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        config.flatten_fields = Some(fields);
    }
    if let Some(toml::Value::Table(env_section)) = section.get("env") {
        let overrides = parse_environment_map(env_section);
        if !overrides.is_empty() {
            config.env_overrides = Some(overrides);
        }
    }
    config
}

fn parse_environment_map(section: &toml::Table) -> BTreeMap<String, EnvConfig> {
    let mut out = BTreeMap::new();
    for (name, value) in section {
        if !is_valid_environment_name(name) {
            continue;
        }
        if let toml::Value::Table(t) = value {
            out.insert(name.clone(), parse_env_config(t));
        }
    }
    out
}

fn parse_env_config(section: &toml::Table) -> EnvConfig {
    let mut config = EnvConfig::default();
    if let Some(toml::Value::String(s)) = section.get("aws-profile") {
        config.aws_profile = Some(s.clone());
    }
    if let Some(toml::Value::String(s)) = section.get("region") {
        config.region = Some(s.clone());
    }
    config
}

/// Pick the configuration for the current git repo (by basename of the repo
/// root), merging on top of the `[defaults]` section. Per-repo values fully
/// replace defaults values (arrays are not concatenated).
pub fn resolve_repo_defaults(settings: &Settings, cwd: &Path) -> (Option<String>, RepoConfig) {
    let Some(root) = git::find_git_root(cwd) else {
        return (None, settings.defaults.clone());
    };
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let repo = settings.repos.get(&name);
    let merged = merge_repo_config(&settings.defaults, repo);
    (Some(name), merged)
}

fn merge_repo_config(base: &RepoConfig, override_: Option<&RepoConfig>) -> RepoConfig {
    let Some(o) = override_ else {
        return base.clone();
    };
    RepoConfig {
        group: o.group.clone().or_else(|| base.group.clone()),
        app: o.app.clone().or_else(|| base.app.clone()),
        flatten_fields: o
            .flatten_fields
            .clone()
            .or_else(|| base.flatten_fields.clone()),
        region: o.region.clone().or_else(|| base.region.clone()),
        env_overrides: o
            .env_overrides
            .clone()
            .or_else(|| base.env_overrides.clone()),
    }
}

/// Resolve the effective env config for a repo + environment by merging the
/// top-level `[env.<env>]` entry with the per-repo `[repo.<repo>.env.<env>]`
/// override on a per-key basis (override wins for keys it sets).
pub fn resolve_env_config(
    settings: &Settings,
    repo_name: Option<&str>,
    env_name: &str,
) -> EnvConfig {
    let base = settings
        .environments
        .get(env_name)
        .cloned()
        .unwrap_or_default();
    let override_ = repo_name
        .and_then(|n| settings.repos.get(n))
        .and_then(|r| r.env_overrides.as_ref())
        .and_then(|m| m.get(env_name));
    let Some(o) = override_ else {
        return base;
    };
    EnvConfig {
        aws_profile: o.aws_profile.clone().or(base.aws_profile),
        region: o.region.clone().or(base.region),
    }
}

/// Template used to seed a fresh `current.insights`. The seeded body keeps
/// `{{ app }}` / `{{ env }}` placeholders intact — they're expanded at query
/// execution time. When no app is configured, the `filter app = …` line is
/// omitted.
pub const DEFAULT_QUERY_TEMPLATE: &str = "fields @timestamp, @message
| sort @timestamp desc
| filter app = '{{ app }}'
| filter level in ['WARN', 'ERROR']
| limit 200";

pub fn default_query(app: Option<&str>) -> String {
    if app.is_none() {
        return DEFAULT_QUERY_TEMPLATE
            .lines()
            .filter(|l| !l.contains("{{ app }}") && !l.contains("{{app}}"))
            .collect::<Vec<_>>()
            .join("\n");
    }
    DEFAULT_QUERY_TEMPLATE.to_string()
}

const DEFAULT_SETTINGS_TEMPLATE: &str = "# cloudwatch-insights settings
#
# Top-level [defaults] applies to every repo. Per-repo overrides live under
# [repo.<repo-name>] and fully replace the corresponding defaults value
# (arrays are not merged).
#
# Supported fields (in either [defaults] or [repo.<name>]):
#   group           = \"/{{ env }}/logs\"  # log group template; {{ env }}, {{ app }} expanded at query time
#   app             = \"my-service\"        # default app filter for the seeded query
#   region          = \"eu-north-1\"        # AWS region fallback when no [env.<name>].region applies
#   flatten-fields  = [\"@message\"]        # JSON-decode these fields and merge their keys into the row
#
# Environments are declared as [env.<name>] (any lower kebab-case name) and may
# set:
#   aws-profile = \"...\"   # exported as AWS_PROFILE for the run
#   region      = \"...\"   # AWS region used for the run
#
# A repo can override individual env keys via [repo.<repo>.env.<env>] — only
# keys you mention are overridden; the rest fall through to the top-level entry.
#
# Example:
#   [defaults]
#   flatten-fields = [\"@message\"]
#
#   [env.systest]
#   aws-profile = \"company-systest\"
#   region      = \"eu-west-1\"
#
#   [env.prod]
#   aws-profile = \"company-prod\"
#   region      = \"us-east-1\"
#
#   [repo.example-service]
#   group = \"/{{ env }}/logs\"
#   app   = \"example-service\"
#
#   [repo.example-service.env.prod]
#   aws-profile = \"example-prod\"   # region inherits from [env.prod]
#
# Run `cloudwatch-insights edit-config` from inside a git repository and a
# commented placeholder section for that repo will be appended below.
";

#[derive(Debug, Default)]
pub struct EnsureResult {
    pub file_created: bool,
    pub added_section: Option<String>,
}

/// Ensure the config file exists, seeding it with a commented template if
/// absent. Additionally, if called from inside a git repository whose
/// basename is not yet a section in the file, append a commented-out
/// placeholder section so the user just has to uncomment and fill in.
pub fn ensure_config_file(path: &Path, cwd: &Path) -> Result<EnsureResult, ConfigError> {
    let mut result = EnsureResult::default();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ConfigError(format!("could not create {}: {}", parent.display(), e))
            })?;
        }
        std::fs::write(path, DEFAULT_SETTINGS_TEMPLATE)
            .map_err(|e| ConfigError(format!("could not write {}: {}", path.display(), e)))?;
        result.file_created = true;
    }

    if let Some(root) = git::find_git_root(cwd) {
        if let Some(name) = root.file_name().and_then(|s| s.to_str()) {
            let contents = std::fs::read_to_string(path)
                .map_err(|e| ConfigError(format!("could not read {}: {}", path.display(), e)))?;
            if !has_repo_section_header(&contents, name) {
                let block = placeholder_section(name);
                let prefix = if contents.is_empty() || contents.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                let mut new = contents;
                new.push_str(prefix);
                new.push_str(&block);
                std::fs::write(path, new).map_err(|e| {
                    ConfigError(format!("could not write {}: {}", path.display(), e))
                })?;
                result.added_section = Some(name.to_string());
            }
        }
    }
    Ok(result)
}

fn has_repo_section_header(contents: &str, name: &str) -> bool {
    let target = format!("[repo.{name}]");
    contents.lines().any(|line| {
        let t = line.trim_start_matches('#').trim();
        t == target
    })
}

fn placeholder_section(repo_name: &str) -> String {
    format!(
        "# {name}: uncomment and fill in the values you want as defaults\n\
         # when running cloudwatch-insights from this repository.\n\
         # [repo.{name}]\n\
         # group = \"/{{{{ env }}}}/logs\"\n\
         # app   = \"{name}\"\n",
        name = repo_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn parses_defaults_and_per_repo() {
        let s = parse_settings(
            "[defaults]\n\
             flatten-fields = [\"@message\"]\n\
             \n\
             [repo.my-service]\n\
             group = \"/{env}/team-x\"\n\
             app = \"my-service\"\n\
             \n\
             [repo.another-service]\n\
             group = \"/prod/another\"\n",
        )
        .unwrap();
        assert_eq!(
            s.defaults.flatten_fields.as_deref(),
            Some(&["@message".to_string()][..])
        );
        let r = &s.repos["my-service"];
        assert_eq!(r.group.as_deref(), Some("/{env}/team-x"));
        assert_eq!(r.app.as_deref(), Some("my-service"));
        assert_eq!(
            s.repos["another-service"].group.as_deref(),
            Some("/prod/another")
        );
    }

    #[test]
    fn empty_yields_empty() {
        let s = parse_settings("").unwrap();
        assert!(s.repos.is_empty());
        assert!(s.environments.is_empty());
    }

    #[test]
    fn ignores_unknown_keys_and_non_table_values() {
        let s = parse_settings("unused = 42\n[repo.foo]\ngroup = \"/foo\"\nextra = 99\n").unwrap();
        assert_eq!(s.repos["foo"].group.as_deref(), Some("/foo"));
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(parse_settings("[broken").is_err());
    }

    #[test]
    fn parses_top_level_env_sections() {
        let s = parse_settings(
            "[env.prod]\n\
             aws-profile = \"company-prod\"\n\
             region = \"us-east-1\"\n\
             \n\
             [env.systest]\n\
             aws-profile = \"company-systest\"\n",
        )
        .unwrap();
        assert_eq!(
            s.environments["prod"],
            EnvConfig {
                aws_profile: Some("company-prod".into()),
                region: Some("us-east-1".into()),
            }
        );
        assert_eq!(
            s.environments["systest"].aws_profile.as_deref(),
            Some("company-systest")
        );
    }

    #[test]
    fn parses_per_repo_env_overrides() {
        let s = parse_settings(
            "[env.prod]\n\
             aws-profile = \"company-prod\"\n\
             region = \"us-east-1\"\n\
             \n\
             [repo.special-service.env.prod]\n\
             aws-profile = \"special-prod\"\n",
        )
        .unwrap();
        let overrides = s.repos["special-service"].env_overrides.as_ref().unwrap();
        assert_eq!(
            overrides["prod"].aws_profile.as_deref(),
            Some("special-prod")
        );
        assert_eq!(overrides["prod"].region, None);
    }

    #[test]
    fn drops_invalid_env_names() {
        let s = parse_settings(
            "[env.PROD]\n\
             aws-profile = \"x\"\n\
             \n\
             [env.us_east_1]\n\
             aws-profile = \"y\"\n\
             \n\
             [env.prod]\n\
             aws-profile = \"ok\"\n",
        )
        .unwrap();
        let mut keys: Vec<&str> = s.environments.keys().map(|k| k.as_str()).collect();
        keys.sort();
        assert_eq!(keys, vec!["prod"]);
    }

    #[test]
    fn validates_environment_names() {
        for ok in ["prod", "p", "us-east-1", "a-b-c", "env1"] {
            assert!(is_valid_environment_name(ok), "{ok} should be valid");
        }
        for bad in ["", "PROD", "us_east", "1prod", "-prod", "prod-", "a--b"] {
            assert!(!is_valid_environment_name(bad), "{bad} should be invalid");
        }
    }

    #[test]
    fn resolve_env_config_merges() {
        let s = parse_settings(
            "[env.prod]\n\
             aws-profile = \"company-prod\"\n\
             region = \"us-east-1\"\n\
             \n\
             [repo.foo.env.prod]\n\
             aws-profile = \"foo-prod\"\n",
        )
        .unwrap();
        let cfg = resolve_env_config(&s, Some("foo"), "prod");
        assert_eq!(cfg.aws_profile.as_deref(), Some("foo-prod"));
        assert_eq!(cfg.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn resolve_env_config_no_settings() {
        let s = parse_settings("").unwrap();
        assert_eq!(
            resolve_env_config(&s, Some("foo"), "prod"),
            EnvConfig::default()
        );
    }

    #[test]
    fn parses_region_in_defaults_and_repo() {
        let s = parse_settings(
            "[defaults]\n\
             region = \"eu-north-1\"\n\
             \n\
             [repo.foo]\n\
             region = \"us-east-1\"\n",
        )
        .unwrap();
        assert_eq!(s.defaults.region.as_deref(), Some("eu-north-1"));
        assert_eq!(s.repos["foo"].region.as_deref(), Some("us-east-1"));
    }

    fn init_repo(parent: &Path, name: &str) -> PathBuf {
        let repo = parent.join(name);
        fs::create_dir(&repo).unwrap();
        let status = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        repo
    }

    #[test]
    fn resolve_repo_defaults_overrides() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "my-service");
        let s = parse_settings(
            "[defaults]\n\
             flatten-fields = [\"@message\"]\n\
             \n\
             [repo.my-service]\n\
             group = \"/{env}/team-x\"\n\
             app = \"my-service\"\n",
        )
        .unwrap();
        let (name, defaults) = resolve_repo_defaults(&s, &repo);
        assert_eq!(name.as_deref(), Some("my-service"));
        assert_eq!(defaults.group.as_deref(), Some("/{env}/team-x"));
        assert_eq!(defaults.app.as_deref(), Some("my-service"));
        assert_eq!(
            defaults.flatten_fields.as_deref(),
            Some(&["@message".to_string()][..])
        );
    }

    #[test]
    fn resolve_repo_defaults_array_full_replace() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "my-service");
        let s = parse_settings(
            "[defaults]\n\
             flatten-fields = [\"@message\"]\n\
             \n\
             [repo.my-service]\n\
             flatten-fields = [\"context\"]\n",
        )
        .unwrap();
        let (_, defaults) = resolve_repo_defaults(&s, &repo);
        assert_eq!(
            defaults.flatten_fields.as_deref(),
            Some(&["context".to_string()][..])
        );
    }

    #[test]
    fn resolve_repo_defaults_no_match() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "unknown-repo");
        let s = parse_settings(
            "[defaults]\n\
             flatten-fields = [\"@message\"]\n",
        )
        .unwrap();
        let (name, defaults) = resolve_repo_defaults(&s, &repo);
        assert_eq!(name.as_deref(), Some("unknown-repo"));
        assert_eq!(
            defaults.flatten_fields.as_deref(),
            Some(&["@message".to_string()][..])
        );
        assert!(defaults.group.is_none());
    }

    #[test]
    fn resolve_repo_defaults_per_repo_region() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "my-service");
        let s = parse_settings(
            "[defaults]\n\
             region = \"eu-north-1\"\n\
             \n\
             [repo.my-service]\n\
             region = \"us-east-1\"\n",
        )
        .unwrap();
        let (_, defaults) = resolve_repo_defaults(&s, &repo);
        assert_eq!(defaults.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn resolve_repo_defaults_inherit_region() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "other-service");
        let s = parse_settings("[defaults]\nregion = \"eu-north-1\"\n").unwrap();
        let (_, defaults) = resolve_repo_defaults(&s, &repo);
        assert_eq!(defaults.region.as_deref(), Some("eu-north-1"));
    }

    #[test]
    fn flatten_fields_filters_non_strings() {
        let s = parse_settings(
            "[defaults]\n\
             flatten-fields = [\"@message\", 42, \"context\"]\n",
        )
        .unwrap();
        assert_eq!(
            s.defaults.flatten_fields.as_deref(),
            Some(&["@message".to_string(), "context".to_string()][..])
        );
    }

    #[test]
    fn default_query_with_app() {
        assert_eq!(
            default_query(Some("my-service")),
            "fields @timestamp, @message\n\
             | sort @timestamp desc\n\
             | filter app = '{{ app }}'\n\
             | filter level in ['WARN', 'ERROR']\n\
             | limit 200"
        );
    }

    #[test]
    fn default_query_without_app() {
        assert_eq!(
            default_query(None),
            "fields @timestamp, @message\n\
             | sort @timestamp desc\n\
             | filter level in ['WARN', 'ERROR']\n\
             | limit 200"
        );
    }

    #[test]
    fn ensure_config_file_creates_and_appends_placeholder() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "my-service");
        let settings_path = root.join("settings.toml");

        let r1 = ensure_config_file(&settings_path, &repo).unwrap();
        assert!(r1.file_created);
        assert_eq!(r1.added_section.as_deref(), Some("my-service"));

        let contents = fs::read_to_string(&settings_path).unwrap();
        assert!(contents.contains("# [repo.my-service]"));
        assert!(contents.contains("# group ="));
        assert!(contents.contains("# app   = \"my-service\""));

        let r2 = ensure_config_file(&settings_path, &repo).unwrap();
        assert!(!r2.file_created);
        assert!(
            r2.added_section.is_none(),
            "placeholder must not be duplicated"
        );
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), contents);
    }

    #[test]
    fn ensure_config_file_no_repo() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let settings_path = root.join("settings.toml");
        let r = ensure_config_file(&settings_path, &root).unwrap();
        assert!(r.file_created);
        assert!(r.added_section.is_none());
        let contents = fs::read_to_string(&settings_path).unwrap();
        // No real (non-comment) section header.
        assert!(!contents.lines().any(|l| l.trim_start().starts_with('[')));
    }

    #[test]
    fn ensure_config_file_skips_when_repo_already_present() {
        let tmp = tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let repo = init_repo(&root, "my-service");
        let settings_path = root.join("settings.toml");
        fs::write(
            &settings_path,
            "[repo.my-service]\ngroup = \"/prod/team\"\n",
        )
        .unwrap();
        let before = fs::read_to_string(&settings_path).unwrap();
        let r = ensure_config_file(&settings_path, &repo).unwrap();
        assert!(!r.file_created);
        assert!(r.added_section.is_none());
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), before);
    }
}
