use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cache;
mod cleaner;
mod commands;
mod env;
mod fs_utils;
mod git;
mod picker;
mod repository;
mod services;
mod task_result;
mod ui;

#[derive(Parser)]
#[command(name = "git-branch-assistant")]
#[command(about = "Helper commands for managing git branches", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect the current repository and suggest actions for local branches.
    Clean {
        /// Path to the git repository (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Dry run mode - analyze without performing actions or prompting
        #[arg(long)]
        dry: bool,
    },
    /// Inspect child directories and highlight git repositories needing attention.
    Repos {
        /// Path to the directory to search (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Dry run mode - analyze without performing actions or prompting
        #[arg(long)]
        dry: bool,
        /// Skip repositories with uncommitted changes
        #[arg(long)]
        skip_dirty_repos: bool,
        /// List every branch across all repos sorted by oldest commit first
        #[arg(long)]
        list: bool,
        /// With --list, prompt to select a branch to check out
        #[arg(short, long, requires = "list")]
        interactive: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Clean { path, dry } => commands::git_clean::run(path, dry)?,
        Command::Repos {
            path,
            dry,
            skip_dirty_repos,
            list,
            interactive,
        } => {
            let exit_code =
                commands::git_repos::run(path, dry, skip_dirty_repos, list, interactive)?;
            std::process::exit(exit_code);
        }
    }

    Ok(())
}
