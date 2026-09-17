use anyhow::Result;

use crate::cli::ListArgs;
use crate::commands::{self, Resolved};
use crate::matcher::normalize;
use crate::output::Rows;

pub fn run(args: &ListArgs) -> Result<()> {
    let loaded = commands::load(&args.common)?;

    let wanted = |resolved: &Resolved| {
        if args.unresolved && resolved.hit.is_some() {
            return false;
        }
        if let Some(merchant) = &args.merchant
            && !normalize(resolved.merchant()).contains(&normalize(merchant))
        {
            return false;
        }
        if let Some(category) = &args.category
            && !normalize(resolved.category()).eq(&normalize(category))
        {
            return false;
        }
        true
    };

    let mut headers = vec!["date", "amount", "merchant", "category", "descriptor"];
    if args.full {
        headers.extend(["value_date", "batch", "balance", "text"]);
    }
    if args.explain {
        headers.push("via");
        headers.push("rule");
    }
    let mut rows = Rows::new(&headers).right_align(&[1]);

    for resolved in loaded.transactions.iter().filter(|r| wanted(r)) {
        let transaction = &resolved.transaction;
        let mut row = vec![
            transaction.booked.to_string(),
            transaction.amount.to_string(),
            resolved.merchant().to_string(),
            resolved.category().to_string(),
            transaction.descriptor.key().to_string(),
        ];
        if args.full {
            row.push(transaction.value_date.to_string());
            row.push(transaction.batch.clone());
            row.push(
                transaction
                    .balance
                    .map(|b| b.to_string())
                    .unwrap_or_default(),
            );
            row.push(transaction.text.clone());
        }
        if args.explain {
            let (via, rule) = match &resolved.hit {
                Some(hit) => (
                    hit.via.clone().unwrap_or_default(),
                    format!("{}: {}", hit.table, hit.rule),
                ),
                None => (String::new(), String::new()),
            };
            row.push(via);
            row.push(rule);
        }
        rows.push(row);
    }

    rows.write(&mut std::io::stdout(), args.output)?;
    if loaded.filtered_out > 0 && matches!(args.output, crate::cli::OutputFormat::Table) {
        eprintln!(
            "\n{} transactions excluded by the date and direction filters",
            loaded.filtered_out
        );
    }
    Ok(())
}
