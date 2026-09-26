//! `kontoutdrag view` — the resolved statements as one JSON document, drawn
//! as charts by the React app under `browser/`.
//!
//! All the aggregation happens in the browser: the document is every
//! transaction, resolved, and the app slices it by date, account and
//! category as the filters change. A few years of a couple of accounts is a
//! few thousand rows, which is nothing to a browser.
//!
//! Each transaction gets a key, which is what a comment written in the view
//! is filed under; see [`crate::comments`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::{Common, ViewArgs};
use crate::commands::{self, Resolved};
use crate::comments::{self, Subject};
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
            comments: comments_file(args)?,
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
    let mut accounts = Vec::new();
    let mut transactions = Vec::new();
    let mut subjects = HashMap::new();
    for (index, path) in args.statements.iter().enumerate() {
        let common = Common {
            statement: path.clone(),
            format: args.format.clone(),
            tables: args.tables.clone(),
            only_tables: args.only_tables,
            from: None,
            to: None,
            spending: false,
            income: false,
        };
        let loaded = commands::load(&common)?;
        let account = account_name(path);
        let mut seen = HashMap::new();
        for resolved in &loaded.transactions {
            let key = key(&account, &resolved.transaction, &mut seen);
            subjects.insert(key.clone(), subject(&key, &account, &resolved.transaction));
            transactions.push(transaction(index, &key, resolved));
        }
        accounts.push(account);
    }
    let json = json!({
        "accounts": accounts,
        "transactions": transactions,
        "commentsFile": comments_file(args)?.display().to_string(),
    })
    .to_string();
    Ok(Built { json, subjects })
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

/// Where comments are kept: the configured statements directory, or else
/// beside the first statement.
pub fn comments_file(args: &ViewArgs) -> Result<PathBuf> {
    let settings = config::load(&crate::paths::config_path())?;
    let directory = match &settings.statements.directory {
        Some(dir) => mapping::expand(dir)?,
        None => args
            .statements
            .first()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    };
    Ok(directory.join(comments::FILE_NAME))
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
    files.extend(args.tables.iter().cloned());
    Ok(files)
}

/// What the account is called in the view: the file name without its
/// extension, and without a `transactions-` prefix if it has one.
fn account_name(path: &Path) -> String {
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
