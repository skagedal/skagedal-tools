//! Marking individual transactions, or a run of them, by hand.
//!
//! The merchant tables answer "who was paid", which is a property of the
//! descriptor and the same every time it appears. Some things are not
//! like that: a week away is a date range, a single transfer is
//! one row on one day, and neither can be expressed as a rule about a
//! string without also catching every other transaction that shares it.
//!
//! A mark selects transactions by date, amount and descriptor, and then
//! sets a category, a merchant name, or tags. Tags are the interesting
//! half — they sit on top of the ordinary category rather than replacing
//! it, so a week away can be totalled without hiding that most of it was
//! food.

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::amount::Amount;
use crate::statement::Transaction;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkFile {
    pub version: u32,
    /// For the reader of the file; the tool does not use them.
    #[serde(default)]
    #[allow(dead_code)]
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: Option<String>,
    #[serde(default)]
    pub marks: Vec<Mark>,
    #[serde(skip)]
    pub origin: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    /// Why this mark exists. Not matched on; it is for reading the file
    /// in a year and for `kontoutdrag marks`.
    #[serde(default)]
    pub note: Option<String>,

    // ---- what it selects ------------------------------------------
    /// A single booking date. Shorthand for `from` and `to` the same day.
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    /// The exact signed amount, written as it appears in the statement.
    #[serde(default)]
    pub amount: Option<String>,
    /// Descriptor keys, exactly. Any one of them is a match — a trip is
    /// one mark listing the places it was spent in, not one mark each.
    #[serde(default)]
    pub descriptor: Vec<String>,
    /// The start of the descriptor key. Any one of them is a match.
    #[serde(default)]
    pub descriptor_prefix: Vec<String>,

    // ---- what it does ---------------------------------------------
    #[serde(default)]
    pub category: Option<String>,
    /// Added to whatever the tables gave, never replacing them.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Overrides the merchant name, for a row the tables cannot name.
    #[serde(default)]
    pub merchant: Option<String>,

    #[serde(skip)]
    parsed_amount: Option<Amount>,
}

impl Mark {
    fn first_day(&self) -> Option<NaiveDate> {
        self.date.or(self.from)
    }
    fn last_day(&self) -> Option<NaiveDate> {
        self.date.or(self.to)
    }

    /// Does this mark apply to the transaction?
    pub fn selects(&self, transaction: &Transaction) -> bool {
        if let Some(first) = self.first_day()
            && transaction.booked < first
        {
            return false;
        }
        if let Some(last) = self.last_day()
            && transaction.booked > last
        {
            return false;
        }
        if let Some(amount) = self.parsed_amount
            && transaction.amount != amount
        {
            return false;
        }
        // The descriptor conditions are one condition between them: any
        // listed descriptor or prefix matching is enough. A trip is one
        // mark naming the places, not one mark for each place.
        if !self.descriptor.is_empty() || !self.descriptor_prefix.is_empty() {
            let key = transaction.descriptor.key().to_uppercase();
            let exact = self.descriptor.iter().any(|d| key == d.to_uppercase());
            let prefixed = self
                .descriptor_prefix
                .iter()
                .any(|p| key.starts_with(&p.to_uppercase()));
            if !exact && !prefixed {
                return false;
            }
        }
        true
    }

    fn has_selector(&self) -> bool {
        self.date.is_some()
            || self.from.is_some()
            || self.to.is_some()
            || self.amount.is_some()
            || !self.descriptor.is_empty()
            || !self.descriptor_prefix.is_empty()
    }

    fn has_effect(&self) -> bool {
        self.category.is_some() || !self.tags.is_empty() || self.merchant.is_some()
    }

    /// How the mark reads in a listing.
    pub fn describe(&self) -> String {
        if let Some(note) = &self.note {
            return note.clone();
        }
        let mut parts = Vec::new();
        match (self.first_day(), self.last_day()) {
            (Some(a), Some(b)) if a == b => parts.push(a.to_string()),
            (Some(a), Some(b)) => parts.push(format!("{a}..{b}")),
            (Some(a), None) => parts.push(format!("from {a}")),
            (None, Some(b)) => parts.push(format!("to {b}")),
            (None, None) => {}
        }
        let mut names: Vec<String> = self.descriptor.clone();
        names.extend(self.descriptor_prefix.iter().map(|p| format!("{p}*")));
        if !names.is_empty() {
            let shown = if names.len() > 3 {
                format!("{} and {} more", names[..3].join(", "), names.len() - 3)
            } else {
                names.join(", ")
            };
            parts.push(shown);
        }
        if let Some(a) = &self.amount {
            parts.push(a.clone());
        }
        parts.join(" ")
    }
}

pub fn parse(yaml: &str, origin: &str) -> Result<MarkFile> {
    let mut file: MarkFile = serde_yaml_ng::from_str(yaml)
        .with_context(|| format!("could not parse the marks file {origin}"))?;
    file.origin = origin.to_string();

    if file.version != SCHEMA_VERSION {
        bail!(
            "{origin}: marks file version {} is not supported (this build reads version {SCHEMA_VERSION})",
            file.version
        );
    }

    for (index, mark) in file.marks.iter_mut().enumerate() {
        let where_ = format!("{origin}: mark {}", index + 1);
        // A mark with nothing to select on would apply to the whole
        // statement, which is never what anyone means.
        if !mark.has_selector() {
            bail!("{where_} has no date, amount or descriptor, so it would select everything");
        }
        if !mark.has_effect() {
            bail!("{where_} sets no category, tags or merchant, so it would do nothing");
        }
        if mark.date.is_some() && (mark.from.is_some() || mark.to.is_some()) {
            bail!("{where_} has both `date` and `from`/`to`; use one or the other");
        }
        if let (Some(from), Some(to)) = (mark.from, mark.to)
            && from > to
        {
            bail!("{where_} has from {from} after to {to}");
        }
        if let Some(raw) = &mark.amount {
            mark.parsed_amount = Some(
                raw.parse::<Amount>()
                    .with_context(|| format!("{where_}: bad amount {raw:?}"))?,
            );
        }
    }
    Ok(file)
}

pub fn load_path(path: &std::path::Path) -> Result<MarkFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    parse(&contents, &path.display().to_string())
}

/// What the marks did to one transaction.
#[derive(Debug, Clone, Default)]
pub struct Marked {
    pub category: Option<String>,
    pub merchant: Option<String>,
    pub tags: Vec<String>,
    /// Indices into the flattened mark list, for `marks` and `explain`.
    pub applied: Vec<usize>,
}

/// Every mark from every file, in the order they were loaded. A later
/// mark's category wins over an earlier one; tags accumulate.
pub struct Marks {
    marks: Vec<Mark>,
    origins: Vec<String>,
}

impl Marks {
    pub fn build(files: &[MarkFile]) -> Marks {
        let mut marks = Vec::new();
        let mut origins = Vec::new();
        for file in files {
            for mark in &file.marks {
                marks.push(mark.clone());
                origins.push(file.origin.clone());
            }
        }
        Marks { marks, origins }
    }

    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.marks.len()
    }

    pub fn get(&self, index: usize) -> (&Mark, &str) {
        (&self.marks[index], self.origins[index].as_str())
    }

    pub fn apply(&self, transaction: &Transaction) -> Marked {
        let mut result = Marked::default();
        for (index, mark) in self.marks.iter().enumerate() {
            if !mark.selects(transaction) {
                continue;
            }
            result.applied.push(index);
            if let Some(category) = &mark.category {
                result.category = Some(category.clone());
            }
            if let Some(merchant) = &mark.merchant {
                result.merchant = Some(merchant.clone());
            }
            for tag in &mark.tags {
                if !result.tags.contains(tag) {
                    result.tags.push(tag.clone());
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statement::{Descriptor, parse_descriptor};

    fn transaction(date: &str, amount: &str, text: &str) -> Transaction {
        Transaction {
            booked: date.parse().unwrap(),
            value_date: date.parse().unwrap(),
            batch: String::new(),
            descriptor: parse_descriptor(text),
            text: text.to_string(),
            amount: amount.parse().unwrap(),
            balance: None,
            reference: None,
            method: None,
        }
    }

    const SAMPLE: &str = r#"
version: 1
name: test
marks:
  - note: A week away
    from: 2023-06-10
    to: 2023-06-17
    tags: [resa-2023]
  - note: One transfer
    date: 2024-03-15
    amount: "-25000.000"
    descriptor: ["10000000002"]
    category: housing
    merchant: Sparkontot
"#;

    #[test]
    fn a_range_mark_tags_everything_inside_it() {
        let marks = Marks::build(&[parse(SAMPLE, "test").unwrap()]);
        let inside = marks.apply(&transaction("2023-06-12", "-260.00", "CAFE"));
        assert_eq!(inside.tags, ["resa-2023"]);
        // Tags do not replace the category.
        assert_eq!(inside.category, None);

        let outside = marks.apply(&transaction("2023-06-18", "-260.00", "CAFE"));
        assert!(outside.tags.is_empty());
    }

    #[test]
    fn a_precise_mark_needs_every_selector_to_agree() {
        let marks = Marks::build(&[parse(SAMPLE, "test").unwrap()]);
        let hit = marks.apply(&transaction("2024-03-15", "-25000.000", "10000000002"));
        assert_eq!(hit.category.as_deref(), Some("housing"));
        assert_eq!(hit.merchant.as_deref(), Some("Sparkontot"));

        // Right day and descriptor, wrong amount.
        let miss = marks.apply(&transaction("2024-03-15", "-1000.000", "10000000002"));
        assert!(miss.category.is_none());
    }

    #[test]
    fn descriptor_matching_ignores_case() {
        let marks = Marks::build(&[parse(
            "version: 1\nmarks:\n  - descriptor_prefix: [example]\n    category: charity\n",
            "test",
        )
        .unwrap()]);
        let hit = marks.apply(&transaction("2025-01-15", "-150.00", "EXAMPLE CHARITY"));
        assert_eq!(hit.category.as_deref(), Some("charity"));
    }

    /// A trip is one mark naming the places it was spent in, so that a
    /// subscription that happens to fall inside the dates is not swept up.
    #[test]
    fn several_descriptors_are_one_condition() {
        let file = parse(
            "version: 1\nmarks:\n  - from: 2025-05-02\n    to: 2025-05-06\n\
             \x20   descriptor_prefix: [HOTELLET, MUSEET, RESTAURANG]\n\
             \x20   tags: [resa-2025]\n",
            "test",
        )
        .unwrap();
        let marks = Marks::build(&[file]);
        for text in ["HOTELLET", "MUSEET", "RESTAURANG X"] {
            assert_eq!(
                marks
                    .apply(&transaction("2025-05-04", "-100.00", text))
                    .tags,
                ["resa-2025"],
                "{text} should have been tagged"
            );
        }
        // Inside the dates, but not one of the places.
        assert!(
            marks
                .apply(&transaction("2025-05-04", "-120.00", "STREAMINGTJANSTEN"))
                .tags
                .is_empty()
        );
    }

    #[test]
    fn refuses_a_mark_that_would_select_everything() {
        let error = parse("version: 1\nmarks:\n  - category: housing\n", "test").unwrap_err();
        assert!(error.to_string().contains("select everything"));
    }

    #[test]
    fn refuses_a_mark_that_would_do_nothing() {
        let error = parse("version: 1\nmarks:\n  - date: 2024-01-01\n", "test").unwrap_err();
        assert!(error.to_string().contains("do nothing"));
    }

    #[test]
    fn refuses_a_backwards_range() {
        let error = parse(
            "version: 1\nmarks:\n  - from: 2024-02-01\n    to: 2024-01-01\n    category: x\n",
            "test",
        )
        .unwrap_err();
        assert!(error.to_string().contains("after to"));
    }

    #[test]
    fn a_later_mark_wins_on_category_but_tags_accumulate() {
        let file = parse(
            "version: 1\nmarks:\n\
             \x20 - date: 2024-01-01\n    category: first\n    tags: [a]\n\
             \x20 - date: 2024-01-01\n    category: second\n    tags: [b]\n",
            "test",
        )
        .unwrap();
        let marked = Marks::build(&[file]).apply(&transaction("2024-01-01", "-1.00", "X"));
        assert_eq!(marked.category.as_deref(), Some("second"));
        assert_eq!(marked.tags, ["a", "b"]);
    }

    #[test]
    fn keeps_the_descriptor_kinds_straight() {
        assert!(matches!(
            parse_descriptor("10000000002"),
            Descriptor::Swish { .. }
        ));
    }
}
