//! Monthly budgets: one YAML file per month, `YYYY-MM.yaml`, in the
//! directory `[budgets] path` names, and the sorting of a month's
//! transactions into the lines of its file.
//!
//! A transaction goes to the single most specific line: one naming the
//! transaction's merchant beats one that does not, then the deepest
//! category that holds the transaction's, then the first in the file.
//! Income lines take transactions whose top-level category is `income`;
//! budget rows take everything else. What no row takes is ignored if it
//! only moves money (see [`category::NOT_SPENDING`]) and is unbudgeted
//! otherwise.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize};

use crate::amount::Amount;
use crate::category;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    month: String,
    #[serde(default)]
    income: Vec<Line>,
    #[serde(default)]
    rows: Vec<Line>,
}

/// One planned amount: a row of spending, or a line of income.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub name: String,
    pub category: String,
    /// Narrows the line to transactions whose resolved merchant is this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant: Option<String>,
    /// Planned, as a positive number: money out for a row, in for income.
    #[serde(deserialize_with = "amount", serialize_with = "amount_f64")]
    pub amount: Amount,
    /// Where the number came from: fixed, average, estimate. Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One month's budget file, read and checked.
#[derive(Debug, Clone)]
pub struct Budget {
    /// `YYYY-MM`, from the file name.
    pub month: String,
    pub path: PathBuf,
    pub income: Vec<Line>,
    pub rows: Vec<Line>,
}

/// Every budget in the directory, oldest first, and what was wrong with
/// the files that were skipped or looked odd.
#[derive(Debug, Default)]
pub struct Budgets {
    pub budgets: Vec<Budget>,
    pub warnings: Vec<String>,
}

impl Budgets {
    pub fn month(&self, month: &str) -> Option<&Budget> {
        self.budgets.iter().find(|b| b.month == month)
    }
}

/// Read every `YYYY-MM.yaml` in the directory. A file that does not parse
/// is skipped with a warning rather than failing the rest.
pub fn load_dir(dir: &Path) -> Budgets {
    let mut out = Budgets::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            out.warnings.push(format!(
                "could not read the budget directory {}: {e}",
                dir.display()
            ));
            return out;
        }
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            month_of_file_name(&name).map(|month| (month.to_string(), entry.path()))
        })
        .collect();
    files.sort();
    for (month, path) in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let parsed = std::fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))
            .and_then(|text| parse(&text, &month, &path));
        match parsed {
            Ok((budget, warnings)) => {
                out.warnings
                    .extend(warnings.into_iter().map(|w| format!("{name}: {w}")));
                out.budgets.push(budget);
            }
            Err(e) => out.warnings.push(format!("{name}: {e:#}")),
        }
    }
    out
}

/// `2026-10` for `2026-10.yaml`; nothing for any other name.
fn month_of_file_name(name: &str) -> Option<&str> {
    let month = name.strip_suffix(".yaml")?;
    is_month(month).then_some(month)
}

pub fn is_month(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || c.is_ascii_digit())
        && matches!(text[5..].parse::<u32>(), Ok(1..=12))
}

/// Parse one file, whose name says it is for `month`. Returns the budget
/// and anything worth warning about that did not stop it loading.
pub fn parse(text: &str, month: &str, path: &Path) -> Result<(Budget, Vec<String>)> {
    let file: File = serde_yaml_ng::from_str(text).context("not a valid budget file")?;
    if file.version != SCHEMA_VERSION {
        bail!(
            "version {} is not supported; this build reads version {SCHEMA_VERSION}",
            file.version
        );
    }
    let mut warnings = Vec::new();
    if file.month != month {
        warnings.push(format!(
            "says month {} but is named for {month}; going by the name",
            file.month
        ));
    }
    for (kind, lines) in [("income", &file.income), ("row", &file.rows)] {
        for line in lines {
            if line.category.trim().is_empty() {
                warnings.push(format!("{kind} {:?} has no category", line.name));
            }
            if line.amount.is_negative() {
                warnings.push(format!(
                    "{kind} {:?} has a negative amount; write amounts as positive numbers",
                    line.name
                ));
            }
        }
    }
    for line in &file.income {
        if category::top(&line.category) != "income" {
            warnings.push(format!(
                "income {:?} is in {:?}, which is not under income, so nothing will reach it",
                line.name, line.category
            ));
        }
    }
    Ok((
        Budget {
            month: month.to_string(),
            path: path.to_path_buf(),
            income: file.income,
            rows: file.rows,
        },
        warnings,
    ))
}

fn amount<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Amount, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Int(i64),
        Float(f64),
        Text(String),
    }
    let text = match Raw::deserialize(deserializer)? {
        Raw::Int(n) => n.to_string(),
        Raw::Float(n) => n.to_string(),
        Raw::Text(s) => s,
    };
    text.parse().map_err(serde::de::Error::custom)
}

fn amount_f64<S: serde::Serializer>(amount: &Amount, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(amount.as_f64())
}

/// What the sorting needs to know about a transaction.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a> {
    pub category: &'a str,
    pub merchant: &'a str,
    pub amount: Amount,
}

/// Where a transaction went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Income(usize),
    Row(usize),
    Unbudgeted,
    Ignored,
}

pub fn assign(budget: &Budget, item: &Item) -> Slot {
    if category::top(item.category) == "income" {
        return match best_line(&budget.income, item) {
            Some(i) => Slot::Income(i),
            None => Slot::Ignored,
        };
    }
    match best_line(&budget.rows, item) {
        Some(i) => Slot::Row(i),
        None if category::is_spending(item.category) => Slot::Unbudgeted,
        None => Slot::Ignored,
    }
}

/// The most specific line that takes the transaction: a matching merchant
/// first, then the deepest category, then the first in the file.
fn best_line(lines: &[Line], item: &Item) -> Option<usize> {
    let mut best: Option<(usize, (bool, usize))> = None;
    for (i, line) in lines.iter().enumerate() {
        if !category::is_within(item.category, &line.category) {
            continue;
        }
        if line.merchant.as_deref().is_some_and(|m| m != item.merchant) {
            continue;
        }
        let rank = (line.merchant.is_some(), category::depth(&line.category));
        if best.is_none_or(|(_, r)| rank > r) {
            best = Some((i, rank));
        }
    }
    best.map(|(i, _)| i)
}

/// A line's actual figure and the transactions behind it, by their index
/// in what was sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    /// Received, for income; spent, for a row or the unbudgeted bucket.
    /// A refund in a budgeted category makes spent smaller.
    pub actual: Amount,
    pub members: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub income: Vec<Tally>,
    pub rows: Vec<Tally>,
    pub unbudgeted: Tally,
}

/// Sort a month's transactions into the budget's lines.
pub fn tally(budget: &Budget, items: &[Item]) -> Outcome {
    let mut out = Outcome {
        income: vec![Tally::default(); budget.income.len()],
        rows: vec![Tally::default(); budget.rows.len()],
        unbudgeted: Tally::default(),
    };
    for (index, item) in items.iter().enumerate() {
        let (tally, sign) = match assign(budget, item) {
            Slot::Income(i) => (&mut out.income[i], item.amount),
            Slot::Row(i) => (&mut out.rows[i], -item.amount),
            Slot::Unbudgeted => (&mut out.unbudgeted, -item.amount),
            Slot::Ignored => continue,
        };
        tally.actual += sign;
        tally.members.push(index);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(text: &str) -> Amount {
        text.parse().unwrap()
    }

    fn budget(yaml: &str) -> Budget {
        parse(yaml, "2031-05", Path::new("2031-05.yaml")).unwrap().0
    }

    fn item<'a>(category: &'a str, merchant: &'a str, amount: &str) -> Item<'a> {
        Item {
            category,
            merchant,
            amount: a(amount),
        }
    }

    const BUDGET: &str = "\
version: 1
month: 2031-05
income:
  - name: Salary
    category: income
    amount: 30000
    basis: fixed
rows:
  - name: Food
    category: food
    amount: 5000
  - name: Groceries
    category: food/groceries
    amount: 4000
    basis: average
  - name: Car
    category: car
    amount: 1500.50
  - name: Gym
    category: health/fitness
    merchant: Some Gym
    amount: 400
  - name: Health
    category: health
    amount: 300
  - name: Saving
    category: transfer/saving
    amount: 2000
";

    #[test]
    fn reads_a_budget_file() {
        let b = budget(BUDGET);
        assert_eq!(b.income.len(), 1);
        assert_eq!(b.rows.len(), 6);
        assert_eq!(b.rows[2].amount, a("1500.50"));
        assert_eq!(b.rows[3].merchant.as_deref(), Some("Some Gym"));
        assert_eq!(b.rows[1].basis.as_deref(), Some("average"));
    }

    #[test]
    fn a_deeper_category_beats_a_shallower_one() {
        let b = budget(BUDGET);
        assert_eq!(
            assign(&b, &item("food/groceries", "ICA", "-100")),
            Slot::Row(1)
        );
        assert_eq!(assign(&b, &item("food/cafe", "", "-50")), Slot::Row(0));
        assert_eq!(assign(&b, &item("food", "", "-50")), Slot::Row(0));
        assert_eq!(assign(&b, &item("car/fuel", "", "-600")), Slot::Row(2));
    }

    #[test]
    fn ancestry_is_by_segment_not_by_prefix() {
        let b = budget(BUDGET);
        assert_eq!(assign(&b, &item("carpets", "", "-900")), Slot::Unbudgeted);
    }

    #[test]
    fn a_matching_merchant_beats_category_alone() {
        let yaml = "version: 1\nmonth: 2031-05\nrows:\n  \
            - {name: Fitness, category: health/fitness, amount: 100}\n  \
            - {name: Gym, category: health, merchant: Some Gym, amount: 400}\n";
        let b = budget(yaml);
        assert_eq!(
            assign(&b, &item("health/fitness", "Some Gym", "-400")),
            Slot::Row(1)
        );
        assert_eq!(
            assign(&b, &item("health/fitness", "Other Gym", "-100")),
            Slot::Row(0)
        );
    }

    #[test]
    fn a_merchant_row_takes_nothing_from_other_merchants() {
        let b = budget(BUDGET);
        assert_eq!(
            assign(&b, &item("health/fitness", "Some Gym", "-400")),
            Slot::Row(3)
        );
        assert_eq!(
            assign(&b, &item("health/fitness", "Another", "-100")),
            Slot::Row(4)
        );
    }

    #[test]
    fn ties_go_to_the_first_in_the_file() {
        let yaml = "version: 1\nmonth: 2031-05\nrows:\n  \
            - {name: One, category: food, amount: 1}\n  \
            - {name: Two, category: food, amount: 2}\n";
        let b = budget(yaml);
        assert_eq!(assign(&b, &item("food", "", "-5")), Slot::Row(0));
    }

    #[test]
    fn income_goes_to_income_lines() {
        let b = budget(BUDGET);
        assert_eq!(
            assign(&b, &item("income/salary", "Employer", "30000")),
            Slot::Income(0)
        );
    }

    #[test]
    fn money_that_only_moves_is_ignored_unless_a_row_asks_for_it() {
        let b = budget(BUDGET);
        assert_eq!(assign(&b, &item("transfer", "", "-500")), Slot::Ignored);
        assert_eq!(assign(&b, &item("refunds", "", "200")), Slot::Ignored);
        assert_eq!(
            assign(&b, &item("transfer/saving", "", "-2000")),
            Slot::Row(5)
        );
        let none = budget("version: 1\nmonth: 2031-05\n");
        assert_eq!(assign(&none, &item("income", "", "100")), Slot::Ignored);
    }

    #[test]
    fn a_refund_reduces_what_was_spent() {
        let b = budget(BUDGET);
        let items = [
            item("food/groceries", "ICA", "-400"),
            item("food/groceries", "ICA", "-250.50"),
            item("food/groceries", "ICA", "50"),
        ];
        let out = tally(&b, &items);
        assert_eq!(out.rows[1].actual, a("600.50"));
        assert_eq!(out.rows[1].members, vec![0, 1, 2]);
    }

    #[test]
    fn what_no_row_takes_is_unbudgeted() {
        let b = budget(BUDGET);
        let items = [
            item("", "", "-120"),
            item("leisure/books", "A Bookshop", "-80"),
            item("transfer", "", "-5000"),
            item("income", "Employer", "30000"),
        ];
        let out = tally(&b, &items);
        assert_eq!(out.unbudgeted.actual, a("200"));
        assert_eq!(out.unbudgeted.members, vec![0, 1]);
        assert_eq!(out.income[0].actual, a("30000"));
    }

    #[test]
    fn reads_only_month_files_from_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("2031-05.yaml"), BUDGET).unwrap();
        std::fs::write(
            dir.path().join("2031-04.yaml"),
            "version: 1\nmonth: 2031-04\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.yaml"), "not: a budget").unwrap();
        std::fs::write(dir.path().join("2031-06.yaml.bak"), "junk").unwrap();
        std::fs::write(dir.path().join("2031-13.yaml"), "junk").unwrap();
        std::fs::write(dir.path().join("README.md"), "# budgets").unwrap();
        let loaded = load_dir(dir.path());
        let months: Vec<&str> = loaded.budgets.iter().map(|b| b.month.as_str()).collect();
        assert_eq!(months, ["2031-04", "2031-05"]);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    }

    #[test]
    fn a_broken_file_is_a_warning_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("2031-05.yaml"), BUDGET).unwrap();
        std::fs::write(dir.path().join("2031-06.yaml"), "version: 1\nrows: [oops").unwrap();
        std::fs::write(
            dir.path().join("2031-07.yaml"),
            "version: 1\nmonth: 2031-07\nrows:\n  - {name: X, category: x, amount: 1, colour: red}\n",
        )
        .unwrap();
        let loaded = load_dir(dir.path());
        assert_eq!(loaded.budgets.len(), 1);
        assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
        assert!(loaded.warnings[0].starts_with("2031-06.yaml: "));
        assert!(loaded.warnings[1].starts_with("2031-07.yaml: "));
    }

    #[test]
    fn warns_about_a_month_that_disagrees_with_the_name() {
        let (b, warnings) = parse(
            "version: 1\nmonth: 2031-06\n",
            "2031-05",
            Path::new("2031-05.yaml"),
        )
        .unwrap();
        assert_eq!(b.month, "2031-05");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn a_missing_directory_is_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_dir(&dir.path().join("nowhere"));
        assert!(loaded.budgets.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
    }
}
