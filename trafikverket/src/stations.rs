//! Station signatures, resolved against the API's own `TrainStation` object
//! rather than hard-coded.
//!
//! The list changes rarely, so it is cached under
//! `~/.cache/skagedal-tools/trafikverket/` and refreshed monthly.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::Client;
use crate::config::TOOL;

/// How long a cached station list is used before it is fetched again.
const MAX_AGE_DAYS: i64 = 30;

/// How many candidates an ambiguous lookup lists before giving up on being
/// helpful.
const MAX_CANDIDATES: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub signature: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stations {
    pub fetched_at: DateTime<Utc>,
    pub stations: Vec<Station>,
}

pub fn cache_path() -> PathBuf {
    skagedal_dirs::cache_dir(TOOL).join("stations.json")
}

/// The station list, from the cache when it is recent enough and from the API
/// otherwise.
pub async fn load(client: &Client, path: &Path, refresh: bool) -> Result<Stations> {
    if !refresh
        && let Some(cached) = read_cache(path)
        && cached.age_days() < MAX_AGE_DAYS
    {
        return Ok(cached);
    }
    let stations = fetch(client).await?;
    write_cache(path, &stations);
    Ok(stations)
}

async fn fetch(client: &Client) -> Result<Stations> {
    let rows = client.stations().await?;
    let mut stations: Vec<Station> = rows
        .into_iter()
        .filter_map(|s| {
            let signature = s.signature?.trim().to_string();
            let name = s.name?.trim().to_string();
            (!signature.is_empty() && !name.is_empty()).then_some(Station { signature, name })
        })
        .collect();
    stations.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.signature.cmp(&b.signature))
    });
    stations.dedup_by(|a, b| a.signature == b.signature);
    if stations.is_empty() {
        bail!("the API returned no stations");
    }
    Ok(Stations {
        fetched_at: Utc::now(),
        stations,
    })
}

/// A cache that is missing, unreadable or written by an older layout is
/// simply not a cache — it is refetched rather than reported.
fn read_cache(path: &Path) -> Option<Stations> {
    let contents = std::fs::read_to_string(path).ok()?;
    let stations: Stations = serde_json::from_str(&contents).ok()?;
    (!stations.stations.is_empty()).then_some(stations)
}

fn write_cache(path: &Path, stations: &Stations) {
    let Ok(json) = serde_json::to_string(stations) else {
        return;
    };
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    // A cache that cannot be written costs a fetch next time, nothing more.
    let _ = std::fs::write(path, json);
}

impl Stations {
    fn age_days(&self) -> i64 {
        (Utc::now() - self.fetched_at).num_days()
    }

    /// Stations whose signature or name contains the query.
    pub fn search(&self, query: &str) -> Vec<&Station> {
        let needle = normalize(query);
        if needle.is_empty() {
            return self.stations.iter().collect();
        }
        self.stations
            .iter()
            .filter(|s| normalize(&s.name).contains(&needle) || normalize(&s.signature) == needle)
            .collect()
    }

    /// Turn what the user typed — a signature or a station name — into a
    /// station. An ambiguous name is an error listing the candidates rather
    /// than a guess.
    pub fn resolve(&self, input: &str) -> Result<&Station> {
        let query = input.trim();
        if query.is_empty() {
            bail!("no station given");
        }
        if let Some(station) = self
            .stations
            .iter()
            .find(|s| normalize(&s.signature) == normalize(query))
        {
            return Ok(station);
        }
        let exact: Vec<&Station> = self
            .stations
            .iter()
            .filter(|s| normalize(&s.name) == normalize(query))
            .collect();
        if let [only] = exact[..] {
            return Ok(only);
        }
        let hits = self.search(query);
        match hits[..] {
            [only] => Ok(only),
            [] => bail!(
                "no station matches {query:?} — try `{TOOL} stations {query}` \
                 or a signature like U or Cst"
            ),
            _ => bail!(
                "{query:?} matches {} stations: {}{}",
                hits.len(),
                hits.iter()
                    .take(MAX_CANDIDATES)
                    .map(|s| format!("{} ({})", s.name, s.signature))
                    .collect::<Vec<_>>()
                    .join(", "),
                if hits.len() > MAX_CANDIDATES {
                    ", …"
                } else {
                    ""
                }
            ),
        }
    }
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn station(signature: &str, name: &str) -> Station {
        Station {
            signature: signature.to_string(),
            name: name.to_string(),
        }
    }

    fn sample() -> Stations {
        Stations {
            fetched_at: Utc::now(),
            stations: vec![
                station("U", "Uppsala C"),
                station("Cst", "Stockholm C"),
                station("Sci", "Stockholm City"),
                station("Arnc", "Arlanda C"),
                station("Knä", "Knivsta"),
            ],
        }
    }

    #[test]
    fn resolves_a_signature_regardless_of_case() {
        assert_eq!(sample().resolve("cst").unwrap().name, "Stockholm C");
        assert_eq!(sample().resolve("U").unwrap().name, "Uppsala C");
    }

    #[test]
    fn an_exact_name_wins_over_the_longer_names_it_prefixes() {
        // "Stockholm C" is also the start of "Stockholm City".
        assert_eq!(sample().resolve("Stockholm C").unwrap().signature, "Cst");
    }

    #[test]
    fn resolves_an_exact_name() {
        assert_eq!(sample().resolve("Stockholm C").unwrap().signature, "Cst");
        assert_eq!(sample().resolve("  uppsala   c ").unwrap().signature, "U");
    }

    #[test]
    fn resolves_an_unambiguous_partial_name() {
        assert_eq!(sample().resolve("knivsta").unwrap().signature, "Knä");
        assert_eq!(sample().resolve("uppsala").unwrap().signature, "U");
    }

    #[test]
    fn an_ambiguous_name_lists_the_candidates() {
        let err = sample().resolve("stockholm").unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("Stockholm C (Cst)"), "{text}");
        assert!(text.contains("Stockholm City (Sci)"), "{text}");
    }

    #[test]
    fn an_unknown_name_says_how_to_look_one_up() {
        let err = sample().resolve("Kabul").unwrap_err();
        assert!(format!("{err:#}").contains("stations Kabul"));
    }

    #[test]
    fn a_signature_wins_over_a_name_that_contains_it() {
        // "U" is a signature and also a substring of several names.
        assert_eq!(sample().resolve("U").unwrap().signature, "U");
    }

    #[test]
    fn search_matches_names_and_signatures() {
        let s = sample();
        assert_eq!(s.search("arlanda").len(), 1);
        assert_eq!(s.search("Arnc")[0].name, "Arlanda C");
        assert_eq!(s.search("").len(), 5);
    }

    #[test]
    fn cache_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("stations.json");
        write_cache(&path, &sample());
        let back = read_cache(&path).unwrap();
        assert_eq!(back.stations.len(), 5);
        assert!(back.age_days() < 1);
    }

    #[test]
    fn a_corrupt_cache_is_ignored() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(read_cache(&path).is_none());
        assert!(read_cache(&dir.path().join("absent.json")).is_none());
    }

    #[test]
    fn an_empty_cache_is_ignored() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let empty = Stations {
            fetched_at: Utc::now(),
            stations: vec![],
        };
        std::fs::write(&path, serde_json::to_string(&empty).unwrap()).unwrap();
        assert!(read_cache(&path).is_none());
    }
}
