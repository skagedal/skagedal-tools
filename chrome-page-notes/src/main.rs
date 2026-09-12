//! Attaches notes in an Obsidian vault to the pages you visit in Chrome.
//!
//! This binary is both the CLI you run (`register`) and the native
//! messaging host Chrome runs (`host`) — see `register.rs` and `host.rs`.

mod config;
mod host;
mod notes;
mod obsidian;
mod register;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Name of this tool's directory under the XDG roots (see `skagedal-dirs`).
pub(crate) const TOOL: &str = "chrome-page-notes";

/// The name the extension passes to `chrome.runtime.sendNativeMessage`,
/// and thus the file name of the host manifest Chrome looks for. Matches
/// `HOST_NAME` in `extension/common.js`.
pub(crate) const HOST_NAME: &str = "tech.skagedal.chrome_page_notes_host";

/// Fixed by the `"key"` field in `extension/manifest.json`, so the ID is
/// the same no matter which directory the extension is loaded from.
pub(crate) const EXTENSION_ID: &str = "jbgofjilflakfjbenbgpppajapiffphn";

/// Attach your own notes, kept in an Obsidian vault, to the pages you
/// visit in Chrome.
#[derive(Parser)]
#[command(name = "chrome-page-notes", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register the native messaging host with Chrome and print how to
    /// load the extension.
    Register {
        /// Load the extension from a checkout rather than the copy built
        /// into this binary, so edits take effect on a reload in Chrome.
        /// Defaults to the directory this binary was built from.
        #[arg(long, value_name = "DIR", num_args = 0..=1,
              default_missing_value = register::SOURCE_EXTENSION_DIR)]
        dev: Option<PathBuf>,
    },
    /// Serve the native messaging protocol on stdin/stdout. Chrome runs
    /// this itself; there's rarely a reason to invoke it by hand.
    Host,
}

fn main() -> ExitCode {
    // Chrome's host manifest has no way to pass arguments — it runs the
    // binary bare and hands it the calling extension's origin as argv[1].
    // That origin is what tells us to serve rather than parse a command
    // line.
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg.starts_with("chrome-extension://"))
    {
        host::run();
        return ExitCode::SUCCESS;
    }

    match Cli::parse().command {
        Commands::Register { dev } => match register::run(dev) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("chrome-page-notes: {e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Host => {
            host::run();
            ExitCode::SUCCESS
        }
    }
}
