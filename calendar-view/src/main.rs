//! Calendars in a window, read through calendula.

mod agenda;
mod calendula;
mod settings;
#[cfg(feature = "web")]
mod sync;
#[cfg(feature = "web")]
mod web;

use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::{Args, Parser, Subcommand};

use agenda::Agenda;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Serve the view and print its URL instead of opening a window, for an
    /// ordinary browser or for linking from another page
    #[arg(long)]
    serve: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the events the view would show, as JSON
    Events(EventsArgs),
    /// Print the calendars and whether each is shown at start, as JSON
    Calendars,
}

#[derive(Debug, Args)]
struct EventsArgs {
    /// First day (YYYY-MM-DD); this week's Monday by default
    #[arg(long)]
    from: Option<NaiveDate>,
    /// Last day, included; six days after --from by default
    #[arg(long)]
    to: Option<NaiveDate>,
    /// Every calendar, not just those shown at start
    #[arg(long)]
    all: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("calendar-view: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let settings = settings::load(&settings::path())?;
    match cli.command {
        Some(Command::Calendars) => {
            let agenda = Agenda::new(settings)?;
            println!("{}", serde_json::to_string_pretty(&agenda.calendars)?);
            Ok(())
        }
        Some(Command::Events(args)) => events(Agenda::new(settings)?, &args),
        None => view(settings, cli.serve),
    }
}

fn events(agenda: Agenda, args: &EventsArgs) -> Result<()> {
    let today = Local::now().date_naive();
    let from = args
        .from
        .unwrap_or(today - Duration::days(today.weekday().num_days_from_monday().into()));
    let to = args.to.unwrap_or(from + Duration::days(6));
    let which = if args.all {
        (0..agenda.calendars.len()).collect()
    } else {
        agenda.shown()
    };
    let overview = agenda.overview(&which, from, to);
    println!("{}", serde_json::to_string_pretty(&overview)?);
    for failure in &overview.failures {
        eprintln!("calendar-view: {}: {}", failure.calendar, failure.error);
    }
    if !overview.failures.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(feature = "web")]
fn view(settings: settings::Settings, serve: bool) -> Result<()> {
    use anyhow::Context;
    let command = settings.sync.clone();
    let every = std::time::Duration::from_secs(60 * settings.sync_minutes.unwrap_or(15).max(1));
    let agenda = Agenda::new(settings).context("reading the calendars")?;
    web::run(agenda, sync::Syncer::start(command, every), serve)
}

#[cfg(not(feature = "web"))]
fn view(_: settings::Settings, _: bool) -> Result<()> {
    anyhow::bail!("built without the window; install with `./install calendar-view`")
}
