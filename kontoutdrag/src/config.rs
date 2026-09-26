//! `settings.toml`: which merchant tables to load, and defaults for the
//! statement files themselves.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::mapping::{self, Table};
use crate::statement::Format;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Merchant tables, in order. A later table wins ties against an
    /// earlier one, so personal tables belong at the bottom.
    #[serde(default, rename = "table")]
    pub tables: Vec<TableSource>,
    #[serde(default)]
    pub statements: Statements,
    /// Files of hand-written marks, applied after the tables.
    #[serde(default, rename = "marks")]
    pub marks: Vec<MarkSource>,
    /// Where the monthly budget files are.
    #[serde(default)]
    pub budgets: Option<BudgetSource>,
}

/// The `[budgets]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetSource {
    /// A directory of `YYYY-MM.yaml` files. `~` and `$VAR` are expanded.
    pub path: String,
}

/// One entry in the `[[marks]]` list.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkSource {
    /// A YAML file on disk. `~` and `$VAR` are expanded.
    pub path: String,
}

impl Default for Settings {
    /// With no configuration at all, the bundled Swedish table is loaded
    /// so the tool does something useful out of the box.
    fn default() -> Self {
        Settings {
            tables: vec![TableSource {
                bundled: Some("se-common".into()),
                path: None,
            }],
            statements: Statements::default(),
            marks: Vec::new(),
            budgets: None,
        }
    }
}

/// One entry in the `[[table]]` list. Exactly one of the two keys.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSource {
    /// A table compiled into the binary, by name.
    #[serde(default)]
    pub bundled: Option<String>,
    /// A YAML file on disk. `~` and `$VAR` are expanded.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Statements {
    /// Export format of the statement files, when `--format` is not given.
    #[serde(default = "default_format")]
    pub format: String,
    /// Directory that a bare statement name is resolved against.
    #[serde(default)]
    pub directory: Option<String>,
}

impl Default for Statements {
    fn default() -> Self {
        Statements {
            format: default_format(),
            directory: None,
        }
    }
}

fn default_format() -> String {
    "seb".into()
}

impl Settings {
    pub fn format(&self) -> Result<Format> {
        self.statements.format.parse()
    }

    /// Resolve a statement argument against the configured directory. An
    /// argument that is already a path, or that exists as given, is used
    /// unchanged.
    pub fn resolve_statement(&self, argument: &Path) -> Result<std::path::PathBuf> {
        if argument.exists() || argument.is_absolute() || argument.components().count() > 1 {
            return Ok(argument.to_path_buf());
        }
        match &self.statements.directory {
            Some(directory) => Ok(mapping::expand(directory)?.join(argument)),
            None => Ok(argument.to_path_buf()),
        }
    }

    /// Load every configured table, in order.
    pub fn load_tables(&self) -> Result<Vec<Table>> {
        let mut tables = Vec::new();
        for source in &self.tables {
            match (&source.bundled, &source.path) {
                (Some(name), None) => tables.push(mapping::load_bundled(name)?),
                (None, Some(path)) => {
                    let path = mapping::expand(path)?;
                    tables.push(mapping::load_path(&path)?);
                }
                (Some(_), Some(_)) => {
                    bail!("a [[table]] entry has both `bundled` and `path`; use one or the other")
                }
                (None, None) => bail!("a [[table]] entry has neither `bundled` nor `path`"),
            }
        }
        if tables.is_empty() {
            bail!(
                "no merchant tables are configured — add a [[table]] entry to {}",
                crate::paths::config_path().display()
            );
        }
        Ok(tables)
    }

    pub fn load_marks(&self) -> Result<Vec<crate::marks::MarkFile>> {
        let mut files = Vec::new();
        for source in &self.marks {
            let path = mapping::expand(&source.path)?;
            files.push(crate::marks::load_path(&path)?);
        }
        Ok(files)
    }

    /// The budget directory, expanded, if one is configured.
    pub fn budget_dir(&self) -> Result<Option<std::path::PathBuf>> {
        self.budgets
            .as_ref()
            .map(|b| mapping::expand(&b.path))
            .transpose()
    }

    pub fn load_budgets(&self) -> Result<Option<crate::budget::Budgets>> {
        Ok(self.budget_dir()?.map(|dir| crate::budget::load_dir(&dir)))
    }
}

/// Read settings from disk. A missing file means the defaults, so the tool
/// works before it is configured.
pub fn load(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    parse(&contents).with_context(|| format!("in {}", path.display()))
}

pub fn parse(toml_str: &str) -> Result<Settings> {
    let settings: Settings = toml::from_str(toml_str).context("could not parse settings.toml")?;
    if settings.tables.is_empty() {
        return Ok(Settings {
            tables: Settings::default().tables,
            ..settings
        });
    }
    Ok(settings)
}

/// What a fresh `settings.toml` is seeded with.
pub const TEMPLATE: &str = include_str!("../settings.example.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_example_config_parses() {
        let settings = parse(TEMPLATE).unwrap();
        assert!(!settings.tables.is_empty());
    }

    #[test]
    fn an_empty_config_still_loads_the_bundled_table() {
        let settings = parse("").unwrap();
        let tables = settings.load_tables().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "se-common");
    }

    #[test]
    fn tables_keep_their_configured_order() {
        let settings =
            parse("[[table]]\nbundled = \"se-common\"\n\n[[table]]\npath = \"/tmp/mine.yaml\"\n")
                .unwrap();
        assert_eq!(settings.tables[0].bundled.as_deref(), Some("se-common"));
        assert_eq!(settings.tables[1].path.as_deref(), Some("/tmp/mine.yaml"));
    }

    #[test]
    fn rejects_a_table_entry_that_says_both() {
        let settings =
            parse("[[table]]\nbundled = \"se-common\"\npath = \"/tmp/x.yaml\"\n").unwrap();
        assert!(settings.load_tables().is_err());
    }

    #[test]
    fn reads_the_budget_directory() {
        let settings = parse("[budgets]\npath = \"/tmp/budget\"\n").unwrap();
        assert_eq!(
            settings.budget_dir().unwrap(),
            Some(std::path::PathBuf::from("/tmp/budget"))
        );
        assert!(parse("[budgets]\ndirectory = \"/tmp/budget\"\n").is_err());
        assert_eq!(parse("").unwrap().budget_dir().unwrap(), None);
    }

    #[test]
    fn rejects_an_unknown_key() {
        assert!(parse("[[table]]\nbundeld = \"typo\"\n").is_err());
    }
}
