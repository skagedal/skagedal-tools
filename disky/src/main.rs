//! disky — move big directories between this machine and a remote, and reclaim
//! the local space once the remote copy has been proved correct.
//!
//! Local/remote pairs are declared in
//! `~/.config/skagedal-tools/disky/config.toml`. Commands find the pair from
//! the current directory, so inside `~/studio` a bare `disky list` shows what
//! is here and what is on the Storage Box.

mod config;
mod disk;
mod ops;
mod rsync;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Which pair to act on. Defaults to the one containing the current directory.
    #[arg(long, short, global = true)]
    pair: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show what exists locally, remotely, or both.
    #[command(alias = "ls")]
    List,

    /// Copy projects to the remote, optionally freeing the local space.
    Offload {
        /// Projects to offload. Defaults to the one containing the current directory.
        projects: Vec<String>,
        /// Delete the local copy — but only after checksum verification passes.
        #[arg(long)]
        delete: bool,
        /// Show what would happen without transferring, verifying, or deleting.
        #[arg(long)]
        dry_run: bool,
    },

    /// Copy projects back from the remote. With no arguments, list what's available.
    Onload {
        projects: Vec<String>,
        /// Sync over an existing local directory instead of refusing.
        #[arg(long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },

    /// Per-project detail, including exactly what differs.
    Status { projects: Vec<String> },

    /// List the configured pairs.
    Pairs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg_path = config::config_path();
    let cfg = config::load(&cfg_path)?;

    if matches!(cli.command, Commands::Pairs) {
        return ops::pairs(&cfg);
    }

    let cwd = std::env::current_dir()?;
    let from_cwd = cfg.pair_for_dir(&cwd);

    let (pair, cwd_project) = match &cli.pair {
        Some(name) => (cfg.resolve(name)?, None),
        None => match from_cwd {
            Some((p, project)) => (p, project),
            None => bail!(
                "{} is not inside any configured pair. Use --pair, or run from one of: {}",
                cwd.display(),
                cfg.pair_names()
            ),
        },
    };

    match cli.command {
        Commands::Pairs => unreachable!("handled above"),
        Commands::List => {
            ops::require_dir(&pair.local)?;
            ops::list(&pair)
        }
        Commands::Offload {
            projects,
            delete,
            dry_run,
        } => {
            let projects = or_cwd_project(projects, cwd_project, "offload")?;
            ops::offload(&pair, &projects, delete, dry_run)
        }
        Commands::Onload {
            projects,
            force,
            dry_run,
        } => ops::onload(&pair, &projects, force, dry_run),
        Commands::Status { projects } => {
            ops::require_dir(&pair.local)?;
            ops::status(&pair, &projects)
        }
    }
}

/// Fall back to the project the current directory is in, so you can `cd` into
/// a project and just say `disky offload --delete`.
fn or_cwd_project(
    projects: Vec<String>,
    cwd_project: Option<String>,
    verb: &str,
) -> Result<Vec<String>> {
    if !projects.is_empty() {
        return Ok(projects);
    }
    let fallback = ops::default_projects(cwd_project);
    if fallback.is_empty() {
        bail!("{verb} needs a project name (try: disky list)");
    }
    Ok(fallback)
}
