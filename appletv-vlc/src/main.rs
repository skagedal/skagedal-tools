//! Plays a local movie file on an Apple TV, using VLC for tvOS.
//!
//! Serves the file over HTTP from this machine, then asks the Apple TV to
//! open it in VLC through VLC's `vlc://` URL scheme. Nothing is transcoded:
//! VLC on tvOS decodes MKV, AC3, DTS and so on natively, which is the whole
//! reason to route around AirPlay.

mod discovery;
mod pairings;
mod play;
mod server;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

/// Play a local movie file on an Apple TV, in VLC.
#[derive(Parser)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    play: PlayArgs,
}

#[derive(Args)]
struct PlayArgs {
    /// The movie file to play.
    #[arg(required = true)]
    file: Option<PathBuf>,

    /// The Apple TV, by its name on the network. Defaults to the only paired one.
    #[arg(long, env = "APPLETV_VLC_DEVICE")]
    device: Option<String>,

    /// The port to serve the movie on.
    #[arg(long, default_value_t = 8010)]
    port: u16,

    /// Only serve the movie and print its URL, to enter under Network Stream in VLC.
    #[arg(long)]
    url_only: bool,
}

#[derive(Subcommand)]
enum Command {
    /// List the Apple TVs on the network.
    Scan,
    /// Pair with an Apple TV, entering the PIN it shows.
    Pair {
        /// The Apple TV, by its name on the network. Defaults to the only one found.
        #[arg(long, env = "APPLETV_VLC_DEVICE")]
        device: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Scan) => play::scan(),
        Some(Command::Pair { device }) => play::pair(device.as_deref()),
        None => play::play(
            cli.play.file.as_deref().expect("clap requires a file"),
            cli.play.device.as_deref(),
            cli.play.port,
            cli.play.url_only,
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
