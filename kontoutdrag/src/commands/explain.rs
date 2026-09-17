use anyhow::Result;

use crate::cli::ExplainArgs;
use crate::commands;
use crate::matcher::normalize;
use crate::statement::parse_descriptor;

pub fn run(args: &ExplainArgs) -> Result<()> {
    let matcher = commands::load_matcher(&args.tables, args.only_tables)?;

    // Accept either a raw statement Text field or a bare descriptor, so
    // you can paste a line straight out of the CSV.
    let descriptor = parse_descriptor(&args.descriptor);
    println!("text        {:?}", args.descriptor);
    println!("kind        {}", descriptor.kind());
    println!("descriptor  {:?}", descriptor.key());
    println!("normalised  {:?}", normalize(descriptor.key()));

    match matcher.lookup(descriptor.key()) {
        Some(hit) => {
            println!("\nmerchant    {}", hit.name);
            println!("category    {}", hit.category.unwrap_or_default());
            if !hit.tags.is_empty() {
                println!("tags        {}", hit.tags.join(", "));
            }
            if let Some(via) = hit.via {
                println!("via         {via}");
            }
            if let Some(note) = hit.note {
                println!("note        {note}");
            }
            println!("matched by  {} in table {}", hit.rule, hit.table);
        }
        None => println!("\nno table matched this descriptor"),
    }

    let candidates = matcher.candidates(descriptor.key());
    if candidates.len() > 1 {
        println!("\nother rules that also match, less specific first:");
        for candidate in candidates.iter().skip(1) {
            println!(
                "  {} — {} in table {}",
                candidate.name, candidate.rule, candidate.table
            );
        }
    }
    Ok(())
}
