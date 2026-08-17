use anyhow::Result;
use clap::{Parser, Subcommand};
use package_json_merge::{install, merge};
use std::path::PathBuf;

const ABOUT: &str = "Git merge driver for package.json. When both branches bump the same \
                     dependency, the higher semver range wins. Other conflicts fall through \
                     to git's default merge.";

#[derive(Parser)]
#[command(name = "package-json-merge", version, about = ABOUT, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the merge (called by git: %O %A %B %P).
    Merge {
        /// ancestor file (%O)
        base: PathBuf,
        /// current file (%A) — written in place
        ours: PathBuf,
        /// other file (%B)
        theirs: PathBuf,
        /// original pathname (%P, optional)
        #[allow(dead_code)]
        path: Option<PathBuf>,
    },
    /// Wire up the merge driver via git config and gitattributes.
    Install {
        /// install globally (~/.gitconfig + global attributes file)
        #[arg(short, long)]
        global: bool,
    },
    /// Remove the merge driver wiring.
    Uninstall {
        /// remove the global wiring instead of the per-repo wiring
        #[arg(short, long)]
        global: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Merge {
            base,
            ours,
            theirs,
            path: _,
        } => {
            let outcome = merge::run_merge(&base, &ours, &theirs)?;
            for r in &outcome.resolutions {
                eprintln!(
                    "package-json-merge: resolved {} ({} + {} → {}) in {}",
                    r.name, r.ours, r.theirs, r.picked, r.section
                );
            }
            if outcome.conflicted {
                if let Some(reason) = &outcome.reason {
                    eprintln!("package-json-merge: falling back ({reason})");
                }
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Install { global } => install::install(global),
        Cmd::Uninstall { global } => install::uninstall(global),
    }
}
