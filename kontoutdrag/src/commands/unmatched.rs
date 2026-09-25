//! The descriptors no table knows about, busiest first. This is the loop:
//! run it, add the top few to a table, run it again.

use std::collections::HashMap;

use anyhow::Result;

use crate::amount::Amount;
use crate::cli::UnmatchedArgs;
use crate::commands;
use crate::matcher::normalize;
use crate::output::Rows;

struct Group {
    descriptor: String,
    kind: &'static str,
    count: usize,
    total: Amount,
    first: chrono::NaiveDate,
    last: chrono::NaiveDate,
}

pub fn run(args: &UnmatchedArgs) -> Result<()> {
    let loaded = commands::load(&args.common)?;
    let considered = loaded.transactions.len();

    let mut groups: HashMap<String, Group> = HashMap::new();
    for resolved in loaded.transactions.iter().filter(|r| !r.is_resolved()) {
        let transaction = &resolved.transaction;
        let key = transaction.descriptor.key();
        let group = groups.entry(normalize(key)).or_insert_with(|| Group {
            descriptor: key.to_string(),
            kind: transaction.descriptor.kind(),
            count: 0,
            total: Amount::ZERO,
            first: transaction.booked,
            last: transaction.booked,
        });
        group.count += 1;
        group.total += transaction.amount;
        group.first = group.first.min(transaction.booked);
        group.last = group.last.max(transaction.booked);
    }

    let mut groups: Vec<Group> = groups
        .into_values()
        .filter(|g| g.count >= args.min_count)
        .collect();
    // Busiest first; ties broken by the larger total, then alphabetically,
    // so the output is stable between runs.
    groups.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.total.cmp(&b.total))
            .then(a.descriptor.cmp(&b.descriptor))
    });
    // Counted before --limit truncates, so the footer reports the size of
    // the work list rather than the size of the page being shown.
    let distinct = groups.len();
    let unmatched: usize = groups.iter().map(|g| g.count).sum();
    if let Some(limit) = args.limit {
        groups.truncate(limit);
    }

    if args.yaml {
        for group in &groups {
            let times = if group.count == 1 { "time" } else { "times" };
            println!("  # seen {} {times}, {} kr", group.count, group.total);
            println!("  - name: {}", yaml_string(&group.descriptor));
            println!("    category: TODO");
            println!("    match:");
            println!("      prefix: [{}]", yaml_string(&group.descriptor));
        }
        return Ok(());
    }

    let mut rows =
        Rows::new(&["count", "total", "kind", "first", "last", "descriptor"]).right_align(&[0, 1]);
    for group in &groups {
        rows.push([
            group.count.to_string(),
            group.total.to_string(),
            group.kind.to_string(),
            group.first.to_string(),
            group.last.to_string(),
            group.descriptor.clone(),
        ]);
    }
    rows.write(&mut std::io::stdout(), args.output)?;

    if matches!(args.output, crate::cli::OutputFormat::Table) {
        let coverage = if considered == 0 {
            100.0
        } else {
            100.0 * (considered - unmatched) as f64 / considered as f64
        };
        eprintln!(
            "\n{unmatched} of {considered} transactions unmatched ({coverage:.1}% resolved), \
             in {distinct} distinct descriptors"
        );
        if groups.len() < distinct {
            eprintln!("showing the {} busiest", groups.len());
        }
    }
    Ok(())
}

/// Quote a YAML scalar unless it is plainly safe bare.
///
/// Descriptors contain `*`, `&` and `:` often enough to matter, and a
/// Swish number is all digits — left bare, `46700000001` comes back as an
/// integer and the table fails to load. Requiring a leading letter is
/// blunt but leaves nothing to reason about.
fn yaml_string(value: &str) -> String {
    let bare_is_safe = value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.')
        && !value.ends_with(' ');
    if bare_is_safe {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}
