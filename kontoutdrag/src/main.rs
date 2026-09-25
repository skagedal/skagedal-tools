//! Identify merchants and categorise spending in a bank statement export.

mod amount;
mod cli;
mod commands;
mod config;
mod mapping;
mod marks;
mod matcher;
mod output;
mod paths;
mod statement;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Subcommand};

fn main() {
    if let Err(error) = run() {
        eprintln!("kontoutdrag: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Subcommand::List(args) => commands::list::run(&args),
        Subcommand::Unmatched(args) => commands::unmatched::run(&args),
        Subcommand::Summary(args) => commands::summary::run(&args),
        Subcommand::Tables(args) => commands::tables::run(&args),
        Subcommand::Marks(args) => commands::marks::run(&args),
        Subcommand::Explain(args) => commands::explain::run(&args),
        Subcommand::EditConfig => commands::edit_config::run(),
    }
}
