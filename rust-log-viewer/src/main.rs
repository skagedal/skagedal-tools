mod app;
mod entry;
mod source;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use crate::app::App;
use crate::source::{SourceSpec, load_entries};

const DESCRIPTION: &str = "View JSONL logs in a TUI (Rust port of log-viewer).";

#[derive(Parser, Debug)]
#[command(name = "rust-log-viewer", about = DESCRIPTION, version)]
struct Cli {
    /// JSONL file to read; use "-" for stdin.
    #[arg(short = 'f', long = "file")]
    file: Option<String>,

    /// Positional file path (alternative to -f).
    path: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let spec = resolve_source(&cli)?;
    let entries = load_entries(&spec).with_context(|| format!("loading {}", spec.label()))?;
    let mut app = App::new(entries, spec.label().to_string());
    ui::run(&mut app)
}

fn resolve_source(cli: &Cli) -> Result<SourceSpec> {
    if let Some(file) = &cli.file {
        if file == "-" {
            return Ok(SourceSpec::Stdin);
        }
        return Ok(SourceSpec::File(PathBuf::from(file)));
    }
    if let Some(path) = &cli.path {
        return Ok(SourceSpec::File(path.clone()));
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Ok(SourceSpec::Stdin);
    }
    anyhow::bail!("no input given — pass a JSONL file (or '-' for stdin), or pipe data in");
}
