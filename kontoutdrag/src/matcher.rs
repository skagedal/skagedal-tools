//! Turning a pile of merchant tables into something that can answer
//! "who is `KVARNBY LIVS`?".

use anyhow::Result;

use crate::mapping::{Match, Merchant, StripPrefix, Table};

/// Normalise a descriptor for matching: upper case, and runs of
/// whitespace collapsed to one space.
///
/// Diacritics are deliberately left alone. Folding Ö to O would make one
/// rule cover both spellings a bank sends, but it also silently merges
/// names that are genuinely different, and it makes a table's rules mean
/// something other than what they say. So a chain that arrives as both
/// `KOPMANS TORG` and `KÖPMANS TORG` gets both patterns written out.
pub fn normalize(text: &str) -> String {
    normalize_inner(text, false)
}

/// Normalise a pattern out of a merchant table.
///
/// Identical to [`normalize`] except that a trailing space is kept. That
/// space is the only word boundary the table syntax has: `prefix: "VT "`
/// means the transit operator and not `VTABERGSKROGEN`, and trimming it —
/// as is right for a descriptor — would quietly turn a careful rule into
/// a greedy one.
pub fn normalize_pattern(text: &str) -> String {
    normalize_inner(text, true)
}

fn normalize_inner(text: &str, keep_trailing_space: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        for upper in ch.to_uppercase() {
            out.push(upper);
        }
    }
    if keep_trailing_space && pending_space {
        out.push(' ');
    }
    out
}

/// How specific a hit was. Ordered, so the best match is the maximum:
/// first by rule kind, then by how much of the descriptor the pattern
/// accounted for, then by table position so a later table wins a tie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Specificity {
    kind: KindRank,
    pattern_length: usize,
    table_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum KindRank {
    Regex = 0,
    Contains = 1,
    Prefix = 2,
    Exact = 3,
}

impl KindRank {
    fn label(self) -> &'static str {
        match self {
            KindRank::Regex => "regex",
            KindRank::Contains => "contains",
            KindRank::Prefix => "prefix",
            KindRank::Exact => "exact",
        }
    }
}

/// A resolved merchant.
#[derive(Debug, Clone)]
pub struct Hit {
    pub name: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    /// The payment provider the descriptor was routed through, when the
    /// merchant was only found after stripping its prefix.
    pub via: Option<String>,
    pub note: Option<String>,
    /// Which table and rule produced the hit, for `--explain`.
    pub table: String,
    pub rule: String,
}

struct Rule {
    kind: KindRank,
    pattern: String,
    regex: Option<regex::Regex>,
    merchant: usize,
    table_index: usize,
}

/// A merchant as loaded, independent of the rules pointing at it.
pub struct Entry {
    pub name: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub table: String,
}

pub struct Matcher {
    rules: Vec<Rule>,
    entries: Vec<Entry>,
    strip_prefixes: Vec<StripPrefix>,
    table_names: Vec<String>,
}

impl Matcher {
    /// Build a matcher from tables in configuration order. Later tables
    /// win ties against earlier ones, so a personal table placed after the
    /// bundled one can override it without editing it.
    pub fn build(tables: &[Table]) -> Result<Matcher> {
        let mut rules = Vec::new();
        let mut entries = Vec::new();
        let mut strip_prefixes: Vec<StripPrefix> = Vec::new();
        let mut table_names = Vec::new();

        for (table_index, table) in tables.iter().enumerate() {
            table_names.push(table.name.clone());
            for prefix in &table.normalize.strip_prefixes {
                if !strip_prefixes.iter().any(|p| p.prefix == prefix.prefix) {
                    strip_prefixes.push(prefix.clone());
                }
            }
            for merchant in &table.merchants {
                let index = entries.len();
                entries.push(Entry {
                    name: merchant.name.clone(),
                    category: merchant.category.clone(),
                    tags: merchant.tags.clone(),
                    note: merchant.note.clone(),
                    table: table.name.clone(),
                });
                push_rules(&mut rules, merchant, index, table_index)?;
            }
        }

        // Longest prefix first, so `KLARNA ` is tried before `K`.
        strip_prefixes.sort_by_key(|p| std::cmp::Reverse(p.prefix.len()));

        Ok(Matcher {
            rules,
            entries,
            strip_prefixes,
            table_names,
        })
    }

    pub fn table_names(&self) -> &[String] {
        &self.table_names
    }

    pub fn merchant_count(&self) -> usize {
        self.entries.len()
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Every loaded merchant, in table order.
    pub fn merchants(&self) -> &[Entry] {
        &self.entries
    }

    /// Look a descriptor up. Tries it as written first; only if nothing
    /// matches does it strip a payment-provider prefix and try the
    /// remainder, so a merchant that genuinely starts with those letters
    /// is not mis-attributed.
    pub fn lookup(&self, descriptor: &str) -> Option<Hit> {
        let normalized = normalize(descriptor);
        if let Some(hit) = self.lookup_normalized(&normalized, None) {
            return Some(hit);
        }
        for strip in &self.strip_prefixes {
            let prefix = normalize_pattern(&strip.prefix);
            if prefix.is_empty() {
                continue;
            }
            if let Some(rest) = normalized.strip_prefix(&prefix) {
                let rest = rest.trim_start();
                if rest.is_empty() {
                    continue;
                }
                if let Some(hit) = self.lookup_normalized(rest, strip.provider.as_deref()) {
                    return Some(hit);
                }
            }
        }
        None
    }

    fn lookup_normalized(&self, normalized: &str, via: Option<&str>) -> Option<Hit> {
        let mut best: Option<(Specificity, &Rule)> = None;
        for rule in &self.rules {
            let Some(length) = rule.matches(normalized) else {
                continue;
            };
            let specificity = Specificity {
                kind: rule.kind,
                pattern_length: length,
                table_index: rule.table_index,
            };
            if best.is_none_or(|(current, _)| specificity > current) {
                best = Some((specificity, rule));
            }
        }
        let (specificity, rule) = best?;
        let entry = &self.entries[rule.merchant];
        Some(Hit {
            name: entry.name.clone(),
            category: entry.category.clone(),
            tags: entry.tags.clone(),
            note: entry.note.clone(),
            via: via.map(str::to_string),
            table: entry.table.clone(),
            rule: format!("{} {:?}", specificity.kind.label(), rule.pattern),
        })
    }

    /// Every merchant that could match the descriptor, best first. Used by
    /// `tables --conflicts` to show where two tables disagree.
    pub fn candidates(&self, descriptor: &str) -> Vec<Hit> {
        let normalized = normalize(descriptor);
        let mut hits: Vec<(Specificity, &Rule)> = self
            .rules
            .iter()
            .filter_map(|rule| {
                let length = rule.matches(&normalized)?;
                Some((
                    Specificity {
                        kind: rule.kind,
                        pattern_length: length,
                        table_index: rule.table_index,
                    },
                    rule,
                ))
            })
            .collect();
        hits.sort_by_key(|(specificity, _)| std::cmp::Reverse(*specificity));
        hits.into_iter()
            .map(|(specificity, rule)| {
                let entry = &self.entries[rule.merchant];
                Hit {
                    name: entry.name.clone(),
                    category: entry.category.clone(),
                    tags: entry.tags.clone(),
                    note: entry.note.clone(),
                    via: None,
                    table: entry.table.clone(),
                    rule: format!("{} {:?}", specificity.kind.label(), rule.pattern),
                }
            })
            .collect()
    }
}

impl Rule {
    /// How many characters of the descriptor the rule accounted for, or
    /// `None` if it did not match.
    fn matches(&self, normalized: &str) -> Option<usize> {
        match self.kind {
            KindRank::Exact => (normalized == self.pattern).then_some(self.pattern.len()),
            KindRank::Prefix => normalized
                .starts_with(&self.pattern)
                .then_some(self.pattern.len()),
            KindRank::Contains => normalized
                .contains(&self.pattern)
                .then_some(self.pattern.len()),
            KindRank::Regex => self
                .regex
                .as_ref()?
                .find(normalized)
                .map(|m| m.as_str().len()),
        }
    }
}

fn push_rules(
    rules: &mut Vec<Rule>,
    merchant: &Merchant,
    merchant_index: usize,
    table_index: usize,
) -> Result<()> {
    let Match {
        exact,
        prefix,
        contains,
        regex,
    } = &merchant.match_;

    let mut plain = |kind: KindRank, patterns: &Vec<String>| {
        for pattern in patterns {
            rules.push(Rule {
                kind,
                pattern: normalize_pattern(pattern),
                regex: None,
                merchant: merchant_index,
                table_index,
            });
        }
    };
    plain(KindRank::Exact, exact);
    plain(KindRank::Prefix, prefix);
    plain(KindRank::Contains, contains);

    for pattern in regex {
        rules.push(Rule {
            kind: KindRank::Regex,
            pattern: pattern.clone(),
            regex: Some(regex::Regex::new(pattern)?),
            merchant: merchant_index,
            table_index,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::parse_table;

    fn matcher(yaml: &str) -> Matcher {
        Matcher::build(&[parse_table(yaml, "test").unwrap()]).unwrap()
    }

    #[test]
    fn normalizes_case_and_whitespace() {
        assert_eq!(normalize("Köpmans Torg"), "KÖPMANS TORG");
        assert_eq!(normalize("  Kvarnby   Livs "), "KVARNBY LIVS");
    }

    /// Two spellings of the same chain stay two strings. A table that
    /// wants both says so; nothing here quietly equates them.
    #[test]
    fn leaves_diacritics_alone() {
        assert_ne!(normalize("KÖPMANS"), normalize("KOPMANS"));

        let m = matcher(
            "version: 1\nname: t\nmerchants:\n\
             - name: Köpmans\n  match:\n    prefix: [KÖPMANS, KOPMANS]\n",
        );
        assert_eq!(m.lookup("KÖPMANS TORG").unwrap().name, "Köpmans");
        assert_eq!(m.lookup("KOPMANS TORG").unwrap().name, "Köpmans");

        let only_folded = matcher(
            "version: 1\nname: t\nmerchants:\n\
             - name: Köpmans\n  match:\n    prefix: [KOPMANS]\n",
        );
        assert!(only_folded.lookup("KÖPMANS TORG").is_none());
    }

    #[test]
    fn a_longer_prefix_beats_a_shorter_one() {
        let m = matcher(
            "version: 1\nname: t\nmerchants:\n\
             - name: Kvarnby\n  match:\n    prefix: [KVARNBY]\n\
             - name: Kvarnby Stormarknad\n  match:\n    prefix: [KVARNBY STOR]\n",
        );
        assert_eq!(m.lookup("KVARNBY LIVS").unwrap().name, "Kvarnby");
        assert_eq!(
            m.lookup("KVARNBY STORMARK").unwrap().name,
            "Kvarnby Stormarknad"
        );
    }

    #[test]
    fn an_exact_rule_beats_a_longer_prefix() {
        let m = matcher(
            "version: 1\nname: t\nmerchants:\n\
             - name: Long prefix\n  match:\n    prefix: [TONLIS]\n\
             - name: Exact\n  match:\n    exact: [TONLISTA]\n",
        );
        assert_eq!(m.lookup("TONLISTA").unwrap().name, "Exact");
    }

    #[test]
    fn a_later_table_overrides_an_earlier_one_on_an_identical_rule() {
        let bundled = parse_table(
            "version: 1\nname: bundled\nmerchants:\n - name: Generic\n   match:\n    prefix: [KVARNBY]\n",
            "a",
        )
        .unwrap();
        let personal = parse_table(
            "version: 1\nname: personal\nmerchants:\n - name: Kvarnby Livs\n   match:\n    prefix: [KVARNBY]\n",
            "b",
        )
        .unwrap();
        let m = Matcher::build(&[bundled, personal]).unwrap();
        let hit = m.lookup("KVARNBY LIVS").unwrap();
        assert_eq!(hit.name, "Kvarnby Livs");
        assert_eq!(hit.table, "personal");
    }

    #[test]
    fn strips_a_provider_prefix_only_when_nothing_matches_as_written() {
        let m = matcher(
            "version: 1\nname: t\n\
             normalize:\n  strip_prefixes:\n    - prefix: 'Z*'\n      provider: Zaldo\n\
             merchants:\n\
             - name: Badrumsbolaget\n  match:\n    prefix: [BADRUMSBOLAGET]\n\
             - name: Zebrakiosken\n  match:\n    prefix: ['Z*ZEBRA']\n",
        );

        let stripped = m.lookup("Z*BADRUMSBOLAGET.SE").unwrap();
        assert_eq!(stripped.name, "Badrumsbolaget");
        assert_eq!(stripped.via.as_deref(), Some("Zaldo"));

        // Matches as written, so the prefix is left alone.
        let direct = m.lookup("Z*ZEBRAKIOSKEN").unwrap();
        assert_eq!(direct.name, "Zebrakiosken");
        assert_eq!(direct.via, None);
    }

    #[test]
    fn regex_handles_the_store_number_prefixes() {
        let m = matcher(
            "version: 1\nname: t\nmerchants:\n\
             - name: Presshörnan\n  match:\n    regex: ['^\\d{6,8} PRESSH']\n",
        );
        assert_eq!(m.lookup("9900001 PRESSH").unwrap().name, "Presshörnan");
        assert!(m.lookup("PRESSHORNAN 4").is_none());
    }

    /// A trailing space in a pattern is the table's only word boundary.
    /// Without it `VT` swallows `VTABERGSKROGEN` and a restaurant bill
    /// lands under public transport.
    #[test]
    fn a_trailing_space_in_a_pattern_is_a_word_boundary() {
        let m = matcher(
            "version: 1\nname: t\nmerchants:\n\
             - name: VT\n  match:\n    prefix: [\"VT \"]\n    exact: [VT]\n",
        );
        assert_eq!(m.lookup("VT APP").unwrap().name, "VT");
        assert_eq!(m.lookup("VT").unwrap().name, "VT");
        assert!(m.lookup("VTABERGSKROGEN").is_none());
    }

    #[test]
    fn a_descriptors_own_trailing_space_is_still_trimmed() {
        assert_eq!(normalize("VT APP  "), "VT APP");
        assert_eq!(normalize_pattern("VT "), "VT ");
        assert_eq!(normalize_pattern("  VT  APP  "), "VT APP ");
    }

    #[test]
    fn an_unknown_descriptor_is_none_rather_than_a_guess() {
        let m =
            matcher("version: 1\nname: t\nmerchants:\n - name: A\n   match:\n    prefix: [AAA]\n");
        assert!(m.lookup("VTABERGET").is_none());
    }
}
