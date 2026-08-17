use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use simons_misc_helpers::format_dependency_changes::{DependencyChanges, write_dependency_changes};
use std::io::{self, IsTerminal, Read};

/// Simon's miscellaneous helpers.
#[derive(Parser)]
#[command(name = "simons-misc-helpers", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pretty-print the JSON output of `git pkgs diff --format=json` (read from stdin).
    FormatDependencyChanges {
        /// When to use colored output.
        #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
        color: ColorChoice,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::FormatDependencyChanges { color } => run_format_dependency_changes(color),
    }
}

fn run_format_dependency_changes(color: ColorChoice) -> Result<()> {
    let use_color = match color {
        ColorChoice::Auto => io::stdout().is_terminal(),
        ColorChoice::Always => true,
        ColorChoice::Never => false,
    };

    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin")?;
    let changes: DependencyChanges =
        serde_json::from_str(&input).context("parsing JSON from stdin")?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_dependency_changes(&mut out, &changes, use_color)?;
    Ok(())
}
