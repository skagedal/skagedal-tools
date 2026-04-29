use std::process::ExitCode;

use clap::Parser;

mod cli;
mod commands;
mod config;
mod console_link;
mod editor;
mod flatten;
mod git;
mod insights;
mod output;
mod paths;
mod query_file;
mod template;
mod terminal;
mod time_range;

use cli::{Cli, Subcommand};

#[tokio::main]
async fn main() -> ExitCode {
    // Rewrite legacy `--env` alias to `--environment` so clap doesn't fight us.
    let argv: Vec<String> = std::env::args()
        .map(|a| {
            if a == "--env" {
                "--environment".into()
            } else if let Some(rest) = a.strip_prefix("--env=") {
                format!("--environment={rest}")
            } else {
                a
            }
        })
        .collect();

    let cli = Cli::parse_from(argv);

    let result: anyhow::Result<()> = match cli.command {
        Subcommand::Query(args) => commands::query::run(args).await,
        Subcommand::Raw(args) => commands::raw::run(args).await,
        Subcommand::Link(args) => commands::link::run(args).await,
        Subcommand::ParseLink(args) => commands::parse_link::run(args).await,
        Subcommand::Show(args) => commands::show::run(args).await,
        Subcommand::EditConfig => commands::edit_config::run().await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Use the chain so that anyhow::Context is preserved.
            let msg = err.to_string();
            if let Some(exit_err) = err.downcast_ref::<commands::ExitError>() {
                eprintln!("error: {}", exit_err.message);
                ExitCode::from(exit_err.code)
            } else {
                eprintln!("error: {msg}");
                ExitCode::from(1)
            }
        }
    }
}
