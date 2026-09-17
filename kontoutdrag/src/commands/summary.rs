use std::collections::HashMap;

use anyhow::Result;

use crate::amount::Amount;
use crate::cli::{GroupBy, SummaryArgs};
use crate::commands::{self, Resolved};
use crate::output::Rows;

pub fn run(args: &SummaryArgs) -> Result<()> {
    let loaded = commands::load(&args.common)?;

    let mut totals: HashMap<String, (usize, Amount)> = HashMap::new();
    for resolved in &loaded.transactions {
        for key in keys(resolved, args.by) {
            let entry = totals.entry(key).or_insert((0, Amount::ZERO));
            entry.0 += 1;
            entry.1 += resolved.transaction.amount;
        }
    }

    let mut rows_data: Vec<(String, usize, Amount)> = totals
        .into_iter()
        .map(|(key, (count, total))| (key, count, total))
        .collect();

    match args.by {
        // Chronological, since a month ordering by size is no use.
        GroupBy::Month => rows_data.sort_by(|a, b| a.0.cmp(&b.0)),
        // Largest outflow first.
        _ => rows_data.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0))),
    }
    if let Some(limit) = args.limit {
        rows_data.truncate(limit);
    }

    let label = match args.by {
        GroupBy::Category => "category",
        GroupBy::Merchant => "merchant",
        GroupBy::Month => "month",
        GroupBy::Tag => "tag",
    };
    let mut rows = Rows::new(&[label, "count", "total"]).right_align(&[1, 2]);
    for (key, count, total) in &rows_data {
        rows.push([key.clone(), count.to_string(), total.to_string()]);
    }

    // A grand total only makes sense when every transaction is counted
    // once, which tags are not.
    if !matches!(args.by, GroupBy::Tag) && args.limit.is_none() && !rows.is_empty() {
        let grand: Amount = loaded
            .transactions
            .iter()
            .map(|r| r.transaction.amount)
            .sum();
        rows.push([
            "(total)".to_string(),
            loaded.transactions.len().to_string(),
            grand.to_string(),
        ]);
    }

    rows.write(&mut std::io::stdout(), args.output)?;
    Ok(())
}

/// The bucket or buckets a transaction counts towards. Only tags can
/// produce more than one, and a transaction with no tags produces none.
fn keys(resolved: &Resolved, by: GroupBy) -> Vec<String> {
    match by {
        GroupBy::Category => vec![non_empty(resolved.category(), "(uncategorised)")],
        GroupBy::Merchant => vec![non_empty(resolved.merchant(), "(unknown)")],
        GroupBy::Month => vec![resolved.transaction.booked.format("%Y-%m").to_string()],
        GroupBy::Tag => resolved
            .hit
            .as_ref()
            .map(|h| h.tags.clone())
            .unwrap_or_default(),
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}
