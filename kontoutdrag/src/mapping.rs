//! The YAML merchant tables: their schema, and loading them from the
//! bundled set or from disk.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use serde::Deserialize;

/// Tables shipped inside the binary. `kontoutdrag tables --bundled` lists
/// them and `--dump` writes one out to start your own from.
static BUNDLED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/merchants");

/// The only schema version this build understands.
pub const SCHEMA_VERSION: u32 = 1;

/// A whole mapping file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub version: u32,
    /// Short identifier, used in `--explain` output and conflict reports.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub normalize: Normalize,
    #[serde(default)]
    pub merchants: Vec<Merchant>,
    /// Where the table was loaded from. Filled in after parsing.
    #[serde(skip)]
    pub origin: String,
}

/// Rewrites applied to a descriptor before matching. Unioned across every
/// loaded table, since these describe the payment system rather than any
/// one person's spending.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Normalize {
    /// Payment-provider prefixes. When a descriptor does not match
    /// anything as written, each prefix is stripped in turn and the
    /// remainder — the actual sub-merchant — is looked up again.
    #[serde(default)]
    pub strip_prefixes: Vec<StripPrefix>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StripPrefix {
    /// The prefix as it appears in the descriptor, before normalisation.
    pub prefix: String,
    /// The provider it identifies, reported as the transaction's `via`.
    /// Null for a prefix that is the bank's own marker rather than a
    /// third party, such as SEB's `WWW ` for card-not-present.
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Merchant {
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(rename = "match", default)]
    pub match_: Match,
}

/// The rule kinds, in decreasing order of how specific a hit is taken to
/// be. Within a kind, a longer pattern wins over a shorter one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    /// The whole normalised descriptor, exactly.
    #[serde(default)]
    pub exact: Vec<String>,
    /// The start of it. The workhorse, because card descriptors are
    /// truncated to twelve characters and so differ only at the end.
    #[serde(default)]
    pub prefix: Vec<String>,
    #[serde(default)]
    pub contains: Vec<String>,
    /// An unanchored regular expression, matched against the normalised
    /// descriptor. Anchor it yourself with `^` when you mean to.
    #[serde(default)]
    pub regex: Vec<String>,
    /// Only transactions of this amount, or within this inclusive range,
    /// signed as in the statement: `"-550"` or `["-600", "-500"]`. For a
    /// descriptor that stands for different things at different amounts —
    /// one landlord billing rent and parking under the same name.
    #[serde(default)]
    pub amount: Option<AmountCondition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AmountCondition {
    Exact(String),
    Range(Vec<String>),
}

impl Match {
    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
            && self.prefix.is_empty()
            && self.contains.is_empty()
            && self.regex.is_empty()
    }
}

/// Parse a table and check the things a typo would otherwise turn into a
/// silently unmatched transaction.
pub fn parse_table(yaml: &str, origin: &str) -> Result<Table> {
    let mut table: Table = serde_yaml_ng::from_str(yaml)
        .with_context(|| format!("could not parse the merchant table {origin}"))?;
    table.origin = origin.to_string();

    if table.version != SCHEMA_VERSION {
        bail!(
            "{origin}: version {} is not supported by this build (expected {SCHEMA_VERSION})",
            table.version
        );
    }
    if table.name.trim().is_empty() {
        bail!("{origin}: the table needs a name");
    }
    for merchant in &table.merchants {
        if merchant.match_.is_empty() {
            bail!(
                "{origin}: merchant {:?} has no match rules, so it can never match anything",
                merchant.name
            );
        }
        for pattern in &merchant.match_.regex {
            regex::Regex::new(pattern).with_context(|| {
                format!("{origin}: merchant {:?} has a bad regex", merchant.name)
            })?;
        }
    }
    Ok(table)
}

/// Load a table from a path, expanding `~` and environment variables.
pub fn load_path(path: &Path) -> Result<Table> {
    let yaml = std::fs::read_to_string(path)
        .with_context(|| format!("could not read the merchant table {}", path.display()))?;
    parse_table(&yaml, &path.display().to_string())
}

/// Load one of the tables compiled into the binary.
pub fn load_bundled(name: &str) -> Result<Table> {
    let file = BUNDLED.get_file(format!("{name}.yaml")).with_context(|| {
        format!(
            "no bundled merchant table named {name:?} (available: {})",
            bundled_names().join(", ")
        )
    })?;
    let yaml = file
        .contents_utf8()
        .context("the bundled table is not valid UTF-8")?;
    parse_table(yaml, &format!("bundled:{name}"))
}

pub fn bundled_names() -> Vec<String> {
    let mut names: Vec<String> = BUNDLED
        .files()
        .filter_map(|f| f.path().file_stem()?.to_str().map(str::to_string))
        .collect();
    names.sort();
    names
}

pub fn bundled_source(name: &str) -> Result<&'static str> {
    BUNDLED
        .get_file(format!("{name}.yaml"))
        .and_then(|f| f.contents_utf8())
        .with_context(|| format!("no bundled merchant table named {name:?}"))
}

/// Expand `~` and `$VAR` in a configured path.
pub fn expand(path: &str) -> Result<PathBuf> {
    let expanded = shellexpand::full(path)
        .with_context(|| format!("could not expand the path {path:?}"))?
        .into_owned();
    Ok(PathBuf::from(expanded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_table_parses() {
        let names = bundled_names();
        assert!(
            names.contains(&"se-common".to_string()),
            "expected the Swedish table to be bundled, found {names:?}"
        );
        for name in names {
            load_bundled(&name).unwrap();
        }
    }

    #[test]
    fn rejects_a_merchant_that_can_never_match() {
        let error = parse_table(
            "version: 1\nname: t\nmerchants:\n  - name: Ghost\n    match: {}\n",
            "test",
        )
        .unwrap_err();
        assert!(error.to_string().contains("no match rules"));
    }

    #[test]
    fn rejects_a_future_schema_version() {
        let error = parse_table("version: 99\nname: t\n", "test").unwrap_err();
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn rejects_an_unknown_key_rather_than_ignoring_it() {
        let error = parse_table(
            "version: 1\nname: t\nmerchants:\n  - name: A\n    catgory: typo\n    match:\n      prefix: [A]\n",
            "test",
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("catgory"), "{error:#}");
    }

    #[test]
    fn rejects_a_bad_regex() {
        let error = parse_table(
            "version: 1\nname: t\nmerchants:\n  - name: A\n    match:\n      regex: ['[unclosed']\n",
            "test",
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("bad regex"), "{error:#}");
    }
}
