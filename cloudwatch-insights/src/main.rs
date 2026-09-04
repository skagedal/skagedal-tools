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
mod pasteboard;
mod paths;
mod progress;
mod query_file;
mod source_command;
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
        Subcommand::CopyLink(args) => commands::copy_link::run(args).await,
        Subcommand::PasteLink(args) => commands::paste_link::run(args).await,
        Subcommand::Show(args) => commands::show::run(args).await,
        Subcommand::EditConfig => commands::edit_config::run().await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if is_broken_pipe(&err) {
                return ExitCode::SUCCESS;
            }
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

fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}
