use anyhow::Result;

use crate::cli::{OutputFormat, TablesArgs};
use crate::commands;
use crate::mapping;
use crate::output::Rows;

pub fn run(args: &TablesArgs) -> Result<()> {
    if let Some(name) = &args.dump {
        print!("{}", mapping::bundled_source(name)?);
        return Ok(());
    }

    if args.bundled {
        for name in mapping::bundled_names() {
            let table = mapping::load_bundled(&name)?;
            println!(
                "{name}  {} merchants  {}",
                table.merchants.len(),
                table.description.unwrap_or_default()
            );
        }
        return Ok(());
    }

    let matcher = commands::load_matcher(&args.tables, args.only_tables)?;

    if args.merchants {
        let mut rows = Rows::new(&["merchant", "category", "table", "note"]);
        for merchant in matcher.merchants() {
            rows.push([
                merchant.name.clone(),
                merchant.category.clone().unwrap_or_default(),
                merchant.table.clone(),
                merchant.note.clone().unwrap_or_default(),
            ]);
        }
        rows.write(&mut std::io::stdout(), OutputFormat::Table)?;
        return Ok(());
    }

    println!(
        "{} tables loaded: {}",
        matcher.table_names().len(),
        matcher.table_names().join(", ")
    );
    println!(
        "{} merchants, {} match rules",
        matcher.merchant_count(),
        matcher.rule_count()
    );
    Ok(())
}
