//! `kontoutdrag view` — the resolved statements as one JSON document, drawn
//! as charts by the React app under `browser/`.
//!
//! Most of the aggregation happens in the browser: the document is every
//! transaction, resolved, and the app slices it by date, account and
//! category as the filters change. A few years of a couple of accounts is a
//! few thousand rows, which is nothing to a browser. The one exception is
//! the budgets, whose lines are filled here, by [`crate::budget`], so that
//! `kontoutdrag budget` and the Budget tab agree.
//!
//! Each transaction gets a key, which is what a comment written in the view
//! is filed under; see [`crate::comments`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::budget::{self, Budgets, Item};
use crate::cli::ViewArgs;
use crate::commands::{self, Resolved};
use crate::comments::Subject;
use crate::config;
use crate::mapping;
use crate::statement::Transaction;

/// The document the view is drawn from, and what each key refers to.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
pub struct Built {
    pub json: String,
    pub subjects: HashMap<String, Subject>,
}

pub fn run(args: &ViewArgs) -> Result<()> {
    if args.json {
        println!("{}", build(args)?.json);
        return Ok(());
    }
    open(args)
}

#[cfg(feature = "web")]
fn open(args: &ViewArgs) -> Result<()> {
    let first = build(args)?;
    let owned = ViewArgs {
        statements: args.statements.clone(),
        format: args.format.clone(),
        tables: args.tables.clone(),
        only_tables: args.only_tables,
        serve: args.serve,
        json: false,
    };
    crate::web::run(
        crate::web::Source {
            build: Box::new(move || build(&owned)),
            watched: watched_files(args)?,
            comments: crate::paths::comments_path(),
        },
        first,
        args.serve,
    )
}

#[cfg(not(feature = "web"))]
fn open(_args: &ViewArgs) -> Result<()> {
    anyhow::bail!(
        "this build was compiled without the `web` feature, so it cannot open a window; \
         rebuild with --features web, or use --json"
    )
}

pub fn build(args: &ViewArgs) -> Result<Built> {
    let settings = config::load(&crate::paths::config_path())?;
    let loaded = commands::load_statements(
        &args.statements,
        &args.format,
        &args.tables,
        args.only_tables,
    )?;
    let mut accounts = Vec::new();
    let mut transactions = Vec::new();
    let mut subjects = HashMap::new();
    let mut keyed = Vec::new();
    for (index, (path, loaded)) in loaded.iter().enumerate() {
        let account = account_name(path);
        let mut seen = HashMap::new();
        for resolved in &loaded.transactions {
            let key = key(&account, &resolved.transaction, &mut seen);
            subjects.insert(key.clone(), subject(&key, &account, &resolved.transaction));
            transactions.push(transaction(index, &key, resolved));
            keyed.push((key, resolved));
        }
        accounts.push(account);
    }
    let budgets = match (settings.budget_dir()?, settings.load_budgets()?) {
        (Some(dir), Some(budgets)) => budgets_json(&dir, &budgets, &keyed),
        _ => Value::Null,
    };
    let json = json!({
        "accounts": accounts,
        "transactions": transactions,
        "commentsFile": crate::paths::comments_path().display().to_string(),
        "budgets": budgets,
    })
    .to_string();
    Ok(Built { json, subjects })
}

/// Each month's budget with its lines filled in: the actual figure and the
/// keys of the transactions behind it. Per month, so a quarter or a year is
/// a sum over several.
fn budgets_json(dir: &Path, budgets: &Budgets, keyed: &[(String, &Resolved)]) -> Value {
    let months: Vec<Value> = budgets
        .budgets
        .iter()
        .map(|b| {
            let in_month: Vec<&(String, &Resolved)> = keyed
                .iter()
                .filter(|(_, r)| commands::month_of(&r.transaction) == b.month)
                .collect();
            let items: Vec<Item> = in_month.iter().map(|(_, r)| item(r)).collect();
            let outcome = budget::tally(b, &items);
            let keys = |tally: &budget::Tally| -> Vec<&str> {
                tally
                    .members
                    .iter()
                    .map(|&i| in_month[i].0.as_str())
                    .collect()
            };
            let lines = |lines: &[budget::Line], tallies: &[budget::Tally]| -> Vec<Value> {
                lines
                    .iter()
                    .zip(tallies)
                    .map(|(line, tally)| {
                        let mut value = serde_json::to_value(line).unwrap_or(Value::Null);
                        value["actual"] = json!(tally.actual.as_f64());
                        value["keys"] = json!(keys(tally));
                        value
                    })
                    .collect()
            };
            json!({
                "month": b.month,
                "file": b.path.display().to_string(),
                "income": lines(&b.income, &outcome.income),
                "rows": lines(&b.rows, &outcome.rows),
                "unbudgeted": {
                    "actual": outcome.unbudgeted.actual.as_f64(),
                    "keys": keys(&outcome.unbudgeted),
                },
            })
        })
        .collect();
    json!({
        "directory": dir.display().to_string(),
        "warnings": budgets.warnings,
        "months": months,
    })
}

pub fn item(resolved: &Resolved) -> Item<'_> {
    Item {
        category: resolved.category(),
        merchant: resolved.merchant(),
        amount: resolved.transaction.amount,
    }
}

/// Stable across reloads, so a comment still finds its row after the
/// statement is re-exported. The bank's reference when there is one;
/// otherwise account, date, amount and text, numbered when the same
/// payment occurs twice in a day.
fn key(account: &str, t: &Transaction, seen: &mut HashMap<String, usize>) -> String {
    if let Some(reference) = &t.reference {
        return format!("{account}:{reference}");
    }
    let base = format!("{account}|{}|{}|{}", t.booked, t.amount, t.text);
    let n = seen.entry(base.clone()).or_insert(0);
    *n += 1;
    if *n == 1 { base } else { format!("{base}|{n}") }
}

fn subject(key: &str, account: &str, t: &Transaction) -> Subject {
    Subject {
        key: key.to_string(),
        account: account.to_string(),
        date: t.booked.format("%Y-%m-%d").to_string(),
        amount: t.amount.to_string(),
        descriptor: t.descriptor.key().to_string(),
        text: t.text.clone(),
        reference: t.reference.clone(),
    }
}

/// Everything the document is built from, so the view can rebuild itself
/// when a rule or a statement changes under it.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
pub fn watched_files(args: &ViewArgs) -> Result<Vec<PathBuf>> {
    let config_path = crate::paths::config_path();
    let settings = config::load(&config_path)?;
    let mut files = vec![config_path];
    for statement in &args.statements {
        files.push(settings.resolve_statement(statement)?);
    }
    for table in &settings.tables {
        if let Some(path) = &table.path {
            files.push(mapping::expand(path)?);
        }
    }
    for marks in &settings.marks {
        files.push(mapping::expand(&marks.path)?);
    }
    // A directory stands for every file in it; see `web::fingerprint`.
    files.extend(settings.budget_dir()?);
    files.extend(args.tables.iter().cloned());
    Ok(files)
}

/// What the account is called in the view: the file name without its
/// extension, and without a `transactions-` prefix if it has one.
pub fn account_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    stem.strip_prefix("transactions-")
        .map(str::to_string)
        .unwrap_or(stem)
}

fn transaction(account: usize, key: &str, resolved: &Resolved) -> Value {
    let t = &resolved.transaction;
    json!({
        "key": key,
        "account": account,
        "date": t.booked.format("%Y-%m-%d").to_string(),
        "amount": t.amount.as_f64(),
        "merchant": resolved.merchant(),
        "category": resolved.category(),
        "tags": resolved.tags(),
        "descriptor": t.descriptor.key(),
        "text": t.text,
        "kind": t.descriptor.kind(),
        "method": t.method,
        "resolved": resolved.is_resolved(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statement::parse_descriptor;

    fn t(date: &str, amount: &str, text: &str, reference: Option<&str>) -> Transaction {
        Transaction {
            booked: date.parse().unwrap(),
            value_date: date.parse().unwrap(),
            batch: String::new(),
            descriptor: parse_descriptor(text),
            text: text.to_string(),
            amount: amount.parse().unwrap(),
            balance: None,
            reference: reference.map(str::to_string),
            method: None,
        }
    }

    #[test]
    fn names_an_account_after_its_file() {
        assert_eq!(
            account_name(Path::new("data/transactions-savings.json")),
            "savings"
        );
        assert_eq!(account_name(Path::new("statement.csv")), "statement");
    }

    #[test]
    fn a_key_prefers_the_banks_reference() {
        let mut seen = HashMap::new();
        assert_eq!(
            key(
                "everyday",
                &t("2026-01-01", "-50", "SHOP", Some("abc")),
                &mut seen
            ),
            "everyday:abc"
        );
    }

    /// Two identical payments on one day are two rows and need two keys.
    #[test]
    fn identical_payments_on_one_day_are_numbered() {
        let mut seen = HashMap::new();
        let row = t("2026-01-01", "-52", "BAR", None);
        assert_eq!(
            key("everyday", &row, &mut seen),
            "everyday|2026-01-01|-52.00|BAR"
        );
        assert_eq!(
            key("everyday", &row, &mut seen),
            "everyday|2026-01-01|-52.00|BAR|2"
        );
    }
}
