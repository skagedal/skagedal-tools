//! Identify merchants and categorise spending in a bank statement export.

mod amount;
mod cli;
mod commands;
// Written and read only by the window, so a build without it leaves them idle.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
mod comments;
mod config;
mod mapping;
mod marks;
mod matcher;
mod output;
mod paths;
mod statement;
#[cfg(feature = "web")]
mod web;

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
        Subcommand::View(args) => commands::view::run(&args),
        Subcommand::EditConfig => commands::edit_config::run(),
    }
}
