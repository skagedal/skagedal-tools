//! `kontoutdrag marks` — what the hand-written marks caught.

use crate::cli::MarksArgs;
use crate::commands;
use crate::output::Rows;
use anyhow::Result;

pub fn run(args: &MarksArgs) -> Result<()> {
    let loaded = commands::load(&args.common)?;

    if loaded.marks.is_empty() {
        println!("no marks are configured — add a [[marks]] entry to the settings file");
        return Ok(());
    }

    let mut rows = Rows::new(&["rows", "effect", "mark", "from"]);
    let mut unused = 0;
    for index in 0..loaded.marks.len() {
        let (mark, origin) = loaded.marks.get(index);
        let hits = loaded.mark_hits[index];
        if hits == 0 {
            unused += 1;
        }
        let mut effect = Vec::new();
        if let Some(category) = &mark.category {
            effect.push(category.clone());
        }
        if let Some(merchant) = &mark.merchant {
            effect.push(format!("= {merchant}"));
        }
        for tag in &mark.tags {
            effect.push(format!("#{tag}"));
        }
        rows.push([
            hits.to_string(),
            effect.join(" "),
            mark.describe(),
            origin.to_string(),
        ]);
    }
    rows.write(&mut std::io::stdout(), args.output)?;

    // A mark that catches nothing is almost always a typo in a date or an
    // amount, and it fails silently otherwise.
    if unused > 0 && matches!(args.output, crate::cli::OutputFormat::Table) {
        eprintln!(
            "\n{unused} mark(s) caught nothing. Within the window given, that may be \
             expected; otherwise check the date, amount and descriptor."
        );
    }
    Ok(())
}
