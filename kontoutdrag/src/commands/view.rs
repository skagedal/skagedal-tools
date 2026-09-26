//! `kontoutdrag view` — the resolved statements as one JSON document, drawn
//! as charts by the React app under `browser/`.
//!
//! All the aggregation happens in the browser: the document is every
//! transaction, resolved, and the app slices it by date, account and
//! category as the filters change. A few years of a couple of accounts is a
//! few thousand rows, which is nothing to a browser.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::{Common, ViewArgs};
use crate::commands::{self, Resolved};

pub fn run(args: &ViewArgs) -> Result<()> {
    let data = build(args)?;
    if args.json {
        println!("{data}");
        return Ok(());
    }
    open(args, data)
}

#[cfg(feature = "web")]
fn open(args: &ViewArgs, data: Value) -> Result<()> {
    crate::web::run(data.to_string(), args.serve)
}

#[cfg(not(feature = "web"))]
fn open(_args: &ViewArgs, _data: Value) -> Result<()> {
    anyhow::bail!(
        "this build was compiled without the `web` feature, so it cannot open a window; \
         rebuild with --features web, or use --json"
    )
}

fn build(args: &ViewArgs) -> Result<Value> {
    let mut accounts = Vec::new();
    let mut transactions = Vec::new();
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
        accounts.push(account_name(path));
        transactions.extend(loaded.transactions.iter().map(|r| transaction(index, r)));
    }
    Ok(json!({ "accounts": accounts, "transactions": transactions }))
}

/// What the account is called in the view: the file name without its
/// extension, and without a `transactions-` prefix if it has one.
fn account_name(path: &std::path::Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    stem.strip_prefix("transactions-")
        .map(str::to_string)
        .unwrap_or(stem)
}

fn transaction(account: usize, resolved: &Resolved) -> Value {
    let t = &resolved.transaction;
    json!({
        "account": account,
        "date": t.booked.format("%Y-%m-%d").to_string(),
        "amount": t.amount.as_f64(),
        "merchant": resolved.merchant(),
        "category": resolved.category(),
        "tags": resolved.tags(),
        "descriptor": t.descriptor.key(),
        "kind": t.descriptor.kind(),
        "resolved": resolved.is_resolved(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn names_an_account_after_its_file() {
        assert_eq!(
            account_name(Path::new("data/transactions-savings.json")),
            "savings"
        );
        assert_eq!(account_name(Path::new("statement.csv")), "statement");
    }
}
