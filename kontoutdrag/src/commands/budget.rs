//! `kontoutdrag budget`: one month's budget file against what the
//! statements say happened. The sorting itself is in [`crate::budget`].

use anyhow::{Result, bail};

use crate::amount::Amount;
use crate::budget::{self, Item, Line, Tally};
use crate::cli::BudgetArgs;
use crate::commands::{self, Resolved};
use crate::config;
use crate::output::Rows;

pub fn run(args: &BudgetArgs) -> Result<()> {
    let settings = config::load(&crate::paths::config_path())?;
    let Some(budgets) = settings.load_budgets()? else {
        bail!(
            "no budget directory is configured — add a [budgets] table with a path to {}",
            crate::paths::config_path().display()
        );
    };
    for warning in &budgets.warnings {
        eprintln!("warning: {warning}");
    }
    let month = match &args.month {
        Some(month) if budget::is_month(month) => month.clone(),
        Some(month) => bail!("--month: {month:?} is not a month of the form YYYY-MM"),
        None => chrono::Local::now().format("%Y-%m").to_string(),
    };
    let Some(budget) = budgets.month(&month) else {
        let have: Vec<&str> = budgets.budgets.iter().map(|b| b.month.as_str()).collect();
        bail!(
            "no budget for {month}; there are budgets for: {}",
            if have.is_empty() {
                "none".to_string()
            } else {
                have.join(", ")
            }
        );
    };

    let loaded = commands::load_statements(
        &args.statements,
        &args.format,
        &args.tables,
        args.only_tables,
    )?;
    let in_month: Vec<(String, &Resolved)> = loaded
        .iter()
        .flat_map(|(path, loaded)| {
            let account = commands::view::account_name(path);
            loaded
                .transactions
                .iter()
                .filter(|r| commands::month_of(&r.transaction) == month)
                .map(move |r| (account.clone(), r))
        })
        .collect();
    let items: Vec<Item> = in_month
        .iter()
        .map(|(_, r)| commands::view::item(r))
        .collect();
    let outcome = budget::tally(budget, &items);

    if args.unbudgeted {
        let mut rows =
            Rows::new(&["date", "account", "amount", "merchant", "category"]).right_align(&[2]);
        for &i in &outcome.unbudgeted.members {
            let (account, r) = &in_month[i];
            rows.push([
                r.transaction.booked.to_string(),
                account.clone(),
                r.transaction.amount.to_string(),
                payee(r),
                r.category().to_string(),
            ]);
        }
        return rows.write(&mut std::io::stdout(), args.output);
    }

    let mut rows =
        Rows::new(&["line", "category", "budget", "actual", "remaining"]).right_align(&[2, 3, 4]);
    let section = |rows: &mut Rows, lines: &[Line], tallies: &[Tally], total: &str| {
        for (line, tally) in lines.iter().zip(tallies) {
            push(rows, &line.name, &line.category, line.amount, tally.actual);
        }
        if !lines.is_empty() {
            push(
                rows,
                total,
                "",
                sum(lines),
                tallies.iter().map(|t| t.actual).sum(),
            );
        }
    };
    section(&mut rows, &budget.income, &outcome.income, "(income)");
    section(&mut rows, &budget.rows, &outcome.rows, "(budgeted)");
    let spent: Amount = outcome.rows.iter().map(|t| t.actual).sum();
    rows.push([
        "(unbudgeted)".to_string(),
        String::new(),
        String::new(),
        outcome.unbudgeted.actual.to_string(),
        String::new(),
    ]);
    push(
        &mut rows,
        "(total spent)",
        "",
        sum(&budget.rows),
        spent + outcome.unbudgeted.actual,
    );
    rows.write(&mut std::io::stdout(), args.output)
}

fn push(rows: &mut Rows, name: &str, category: &str, planned: Amount, actual: Amount) {
    rows.push([
        name.to_string(),
        category.to_string(),
        planned.to_string(),
        actual.to_string(),
        (planned + -actual).to_string(),
    ]);
}

fn sum(lines: &[Line]) -> Amount {
    lines.iter().map(|l| l.amount).sum()
}

fn payee(r: &Resolved) -> String {
    match r.merchant() {
        "" => r.transaction.descriptor.key().to_string(),
        name => name.to_string(),
    }
}
