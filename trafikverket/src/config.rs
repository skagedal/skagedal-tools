//! `config.toml`: the API key and the named routes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::ticket::Ticket;

/// The name this tool's directories are namespaced under.
pub const TOOL: &str = "trafikverket";

/// The environment variable that overrides the configured API key.
pub const API_KEY_ENV: &str = "TRAFIKVERKET_API_KEY";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub api_key: Option<String>,
    pub default_route: Option<String>,
    #[serde(default)]
    pub route: BTreeMap<String, Route>,
}

/// Two stations and, optionally, the products a ticket for them covers.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Route {
    pub from: String,
    pub to: String,
    /// Train products the ticket for this route covers. Omit to report every
    /// train regardless of product.
    pub products: Option<Vec<String>>,
}

impl Route {
    pub fn ticket(&self) -> Ticket {
        Ticket {
            products: self.products.clone(),
        }
    }
}

/// `~/.config/skagedal-tools/trafikverket/config.toml`.
pub fn config_path() -> PathBuf {
    skagedal_dirs::config_dir(TOOL).join("config.toml")
}

/// Read the configuration. A missing file is an empty configuration, not an
/// error — the tool still works from `--from`/`--to` and the environment.
pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    parse(&contents).with_context(|| format!("could not parse {}", path.display()))
}

pub fn parse(contents: &str) -> Result<Config> {
    Ok(toml::from_str(contents)?)
}

impl Config {
    /// The API key, preferring the environment over the file so a key can be
    /// supplied per invocation.
    pub fn api_key(&self) -> Option<String> {
        std::env::var(API_KEY_ENV)
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .or_else(|| self.api_key.clone())
    }

    /// Pick the route to report on: the one named, else the configured
    /// default, else the only one there is.
    pub fn resolve_route(&self, name: Option<&str>) -> Result<(&str, &Route)> {
        if let Some(name) = name {
            return self
                .find(name)
                .with_context(|| format!("no route named {name:?} in the configuration"));
        }
        if let Some(default) = self.default_route.as_deref() {
            return self.find(default).with_context(|| {
                format!("default-route is {default:?}, but no such route is configured")
            });
        }
        let mut routes = self.route.iter();
        match (routes.next(), routes.next()) {
            (Some((name, route)), None) => Ok((name.as_str(), route)),
            (None, _) => bail!(
                "no routes configured — add one to {}, or give --from and --to",
                config_path().display()
            ),
            _ => bail!(
                "several routes configured ({}) — name one with --route, or set default-route",
                self.route_names().join(", ")
            ),
        }
    }

    fn find(&self, name: &str) -> Result<(&str, &Route)> {
        match self.route.get_key_value(name) {
            Some((name, route)) => Ok((name.as_str(), route)),
            None if self.route.is_empty() => bail!("no routes are configured"),
            None => bail!("configured routes: {}", self.route_names().join(", ")),
        }
    }

    pub fn route_names(&self) -> Vec<&str> {
        self.route.keys().map(String::as_str).collect()
    }
}

/// Written when no configuration file exists yet, so there is something to
/// edit rather than a blank page.
pub const TEMPLATE: &str = r##"# trafikverket — the next trains between two stations, and how late they are.
#
# The data comes from Trafikverket's open API. A key is free: register in
# Trafikverket's data portal at https://data.trafikverket.se, then create a key
# under your account. Put it here, or in $TRAFIKVERKET_API_KEY, which takes
# precedence. (api.trafikinfo.trafikverket.se is the API endpoint itself; there
# is nothing to sign up for there.)

# api-key = "..."

# The route reported when none is named with --route. Not needed when there
# is only one route.
# default-route = "commute"

# A route is two station signatures plus, optionally, the train products the
# ticket for that route covers. Signatures come from the API's own TrainStation
# object: run `trafikverket stations uppsala` to look one up. Leave `products`
# out and every train is reported.
#
# The example is a Movingo season ticket for the single route Uppsala to
# Stockholm. That ticket covers Mälartåg and SJ Regional; it does not cover
# SJ Snabbtåg or the night trains. SJ InterCity on the Linköping–Uppsala–Tierp
# line needs the "Alla sträckor" ticket, so add "SJ InterCity" to the list
# below only if that is the ticket you hold.
#
# [route.commute]
# from = "U"
# to = "Cst"
# products = ["Mälartåg", "SJ Regional"]
"##;

/// Create the configuration file with the commented template if it is not
/// there. Returns true when it was created.
pub fn ensure_file(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(path, TEMPLATE)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const SAMPLE: &str = r#"
api-key = "abc123"
default-route = "commute"

[route.commute]
from = "U"
to = "Cst"
products = ["Mälartåg", "SJ Regional"]

[route.weekend]
from = "Cst"
to = "G"
"#;

    #[test]
    fn parses_routes_and_key() {
        let config = parse(SAMPLE).unwrap();
        assert_eq!(config.api_key.as_deref(), Some("abc123"));
        let (name, route) = config.resolve_route(None).unwrap();
        assert_eq!(name, "commute");
        assert_eq!(route.from, "U");
        assert_eq!(route.to, "Cst");
        assert_eq!(
            route.ticket(),
            Ticket::for_products(["Mälartåg", "SJ Regional"])
        );
    }

    #[test]
    fn a_route_without_products_is_unrestricted() {
        let config = parse(SAMPLE).unwrap();
        let (_, route) = config.resolve_route(Some("weekend")).unwrap();
        assert!(route.ticket().is_unrestricted());
    }

    #[test]
    fn an_unknown_route_lists_the_configured_ones() {
        let config = parse(SAMPLE).unwrap();
        let err = config.resolve_route(Some("nope")).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("nope"), "{text}");
        assert!(text.contains("commute, weekend"), "{text}");
    }

    #[test]
    fn a_single_route_needs_no_default() {
        let config = parse("[route.only]\nfrom = \"U\"\nto = \"Cst\"\n").unwrap();
        let (name, _) = config.resolve_route(None).unwrap();
        assert_eq!(name, "only");
    }

    #[test]
    fn several_routes_without_a_default_ask_for_one() {
        let config =
            parse("[route.a]\nfrom = \"U\"\nto = \"Cst\"\n[route.b]\nfrom = \"Cst\"\nto = \"U\"\n")
                .unwrap();
        let err = config.resolve_route(None).unwrap_err();
        assert!(format!("{err:#}").contains("--route"));
    }

    #[test]
    fn no_routes_at_all_points_at_the_config_file() {
        let config = parse("").unwrap();
        let err = config.resolve_route(None).unwrap_err();
        assert!(format!("{err:#}").contains("--from"));
    }

    #[test]
    fn a_missing_file_is_an_empty_configuration() {
        let dir = tempdir().unwrap();
        let config = load(&dir.path().join("nothing.toml")).unwrap();
        assert!(config.route.is_empty());
    }

    #[test]
    fn malformed_toml_names_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[route.a\n").unwrap();
        let err = load(&path).unwrap_err();
        assert!(format!("{err:#}").contains("config.toml"));
    }

    #[test]
    fn ensure_file_seeds_a_template_once() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        assert!(ensure_file(&path).unwrap());
        assert!(!ensure_file(&path).unwrap());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[route.commute]"));
        // The template must parse, so that uncommenting a block is enough.
        assert!(parse(&contents).unwrap().route.is_empty());
    }
}
