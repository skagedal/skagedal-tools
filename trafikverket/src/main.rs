//! `trafikverket` — the next trains between two stations, and how late they
//! are, from Trafikverket's open API.

use std::io::{BufRead, IsTerminal};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Days, FixedOffset, Local, NaiveTime, TimeZone};
use clap::Parser;

mod api;
mod cli;
mod config;
mod journeys;
mod keychain;
mod model;
mod output;
mod query;
mod stations;
mod ticket;

use cli::{
    AuthAction, AuthArgs, Cli, Command, ConfigAction, ConfigArgs, NextArgs, RawArgs, StationsArgs,
};
use config::Config;
use ticket::Ticket;

/// How far back to look for departures. A train whose advertised time has
/// passed but whose forecast has not is still one you can catch.
const LOOKBACK_MINUTES: i64 = 30;

/// How much longer than the departure window to ask for arrivals, so that the
/// far end of a trip leaving at the edge of the window is still in the answer.
const ARRIVAL_ALLOWANCE_MINUTES: i64 = 4 * 60;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Stations(args)) => run_stations(args).await,
        Some(Command::Config(args)) => run_config(args),
        Some(Command::Auth(args)) => run_auth(args).await,
        Some(Command::Raw(args)) => run_raw(args).await,
        None => run_next(cli.next).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run_next(args: NextArgs) -> Result<()> {
    let config = config::load(&config::config_path())?;
    let client = api::Client::new(require_api_key(&config)?)?;

    let (from_input, to_input, mut ticket) = match (&args.from, &args.to) {
        // `--from`/`--to` name a route the configuration says nothing about,
        // so no product list from the file is carried over to it: a ticket
        // for one route says nothing about another.
        (Some(from), Some(to)) => (from.clone(), to.clone(), Ticket::unrestricted()),
        _ => {
            let (_, route) = config.resolve_route(args.route.as_deref())?;
            (route.from.clone(), route.to.clone(), route.ticket())
        }
    };
    if args.any_product {
        ticket = Ticket::unrestricted();
    } else if !args.products.is_empty() {
        ticket = Ticket::for_products(args.products.clone());
    }

    let stations = stations::load(&client, &stations::cache_path(), false).await?;
    let mut from = stations
        .resolve(&from_input)
        .with_context(|| format!("could not resolve the origin {from_input:?}"))?;
    let mut to = stations
        .resolve(&to_input)
        .with_context(|| format!("could not resolve the destination {to_input:?}"))?;
    if args.reverse {
        std::mem::swap(&mut from, &mut to);
    }
    if from.signature == to.signature {
        bail!("{} is both the origin and the destination", from.name);
    }

    // The window the API is asked for is expressed in minutes from now, so
    // `--at` becomes an offset rather than a second notion of the clock.
    let now = Local::now().fixed_offset();
    let reference = match args.at {
        Some(time) => next_occurrence(&Local, now, time)?,
        None => now,
    };
    let offset = (reference - now).num_minutes();

    let (departures, arrivals) = tokio::try_join!(
        client.announcements(
            &from.signature,
            query::DEPARTURE,
            offset - LOOKBACK_MINUTES,
            offset + args.window,
        ),
        client.announcements(
            &to.signature,
            query::ARRIVAL,
            offset - LOOKBACK_MINUTES,
            offset + args.window + ARRIVAL_ALLOWANCE_MINUTES,
        ),
    )?;

    let selection = journeys::select(
        journeys::build(&departures, &arrivals, &ticket),
        reference,
        args.count as usize,
        args.all,
    );

    let report = output::Report {
        from: output::Endpoint {
            signature: &from.signature,
            name: &from.name,
        },
        to: output::Endpoint {
            signature: &to.signature,
            name: &to.name,
        },
        now,
        reference,
        window_minutes: args.window,
        ticket: &ticket,
        selection: &selection,
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::render_json(&report))?
        );
    } else {
        print!("{}", output::render(&report, console::colors_enabled()));
    }
    Ok(())
}

/// The next instant at which the clock in `zone` reads `time`: today if that is
/// still to come, otherwise tomorrow.
///
/// Resolved through the zone rather than through `now`'s own offset, so that
/// the two nights a year the offset changes don't move the answer by an hour.
/// The hour that DST skips has no instant at all; there the next day's is used.
fn next_occurrence<Tz: TimeZone>(
    zone: &Tz,
    now: DateTime<FixedOffset>,
    time: NaiveTime,
) -> Result<DateTime<FixedOffset>> {
    let mut date = now.with_timezone(zone).date_naive();
    for _ in 0..2 {
        let moment = zone
            .from_local_datetime(&date.and_time(time))
            .earliest()
            .map(|m| m.fixed_offset());
        if let Some(moment) = moment
            && moment > now
        {
            return Ok(moment);
        }
        date = date
            .checked_add_days(Days::new(1))
            .context("the day after today is out of range")?;
    }
    bail!("the clock does not reach {time} in the next two days")
}

async fn run_stations(args: StationsArgs) -> Result<()> {
    let config = config::load(&config::config_path())?;
    let client = api::Client::new(require_api_key(&config)?)?;
    let stations = stations::load(&client, &stations::cache_path(), args.refresh).await?;

    let query = args.query.as_deref().unwrap_or("");
    let hits = stations.search(query);
    if hits.is_empty() {
        bail!("no station matches {query:?}");
    }
    let width = hits
        .iter()
        .map(|s| s.signature.chars().count())
        .max()
        .unwrap_or(0);
    for station in hits {
        println!("{:width$}  {}", station.signature, station.name);
    }
    Ok(())
}

async fn run_raw(args: RawArgs) -> Result<()> {
    let config = config::load(&config::config_path())?;
    let client = api::Client::new(require_api_key(&config)?)?;

    let body = match (&args.object, &args.query) {
        (Some(objecttype), _) => {
            let schema = match args.schema.as_deref() {
                Some(schema) => schema.to_string(),
                None => query::default_schema(objecttype)
                    .with_context(|| {
                        format!("no default schema version for {objecttype} — give --schema")
                    })?
                    .to_string(),
            };
            client
                .raw_object(objecttype, &schema, args.limit, args.filter.as_deref())
                .await?
        }
        (None, Some(path)) => {
            let document = if path == "-" {
                std::io::read_to_string(std::io::stdin())
                    .context("could not read the query from stdin")?
            } else {
                std::fs::read_to_string(path).with_context(|| format!("could not read {path}"))?
            };
            client.raw(&document).await?
        }
        // clap's argument group makes one of the two mandatory.
        (None, None) => unreachable!("--object or --query is required"),
    };

    if args.pretty {
        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value)?),
            Err(_) => println!("{body}"),
        }
    } else {
        println!("{body}");
    }
    Ok(())
}

fn run_config(args: ConfigArgs) -> Result<()> {
    let path = config::config_path();
    match args.action {
        Some(ConfigAction::Path) => {
            println!("{}", path.display());
            Ok(())
        }
        Some(ConfigAction::Edit) => {
            if config::ensure_file(&path)? {
                eprintln!("created {}", path.display());
            }
            edit(&path)
        }
        None => {
            let config = config::load(&path)?;
            println!("{}", path.display());
            if !path.exists() {
                println!("(not created yet — run `trafikverket config edit`)");
                return Ok(());
            }
            match config.api_key()? {
                Some((_, source)) => println!("api key: set, from {}", source.describe()),
                None => println!("api key: missing — run `trafikverket auth`"),
            }
            let names = config.route_names();
            if names.is_empty() {
                println!("routes: none");
            } else {
                println!("routes: {}", names.join(", "));
            }
            if let Some(default) = config.default_route.as_deref() {
                println!("default route: {default}");
            }
            Ok(())
        }
    }
}

fn edit(path: &std::path::Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor)
        .arg(path)
        .status()
        .with_context(|| format!("could not run {editor}"))?;
    if !status.success() {
        bail!("{editor} exited with {status}");
    }
    Ok(())
}

fn require_api_key(config: &Config) -> Result<String> {
    match config.api_key()? {
        Some((key, _)) => Ok(key),
        None => bail!(
            "no API key. Get one free from Trafikverket's data portal at \
             https://data.trafikverket.se, then run `trafikverket auth` to put it in \
             the keychain (or set ${})",
            config::API_KEY_ENV
        ),
    }
}

async fn run_auth(args: AuthArgs) -> Result<()> {
    let config = config::load(&config::config_path())?;
    match args.action {
        Some(AuthAction::Status) => auth_status(&config),
        Some(AuthAction::Forget) => auth_forget(),
        None => auth_set(&config, args.no_verify).await,
    }
}

/// Ask for the key, check that Trafikverket accepts it, and file it in the
/// keychain.
async fn auth_set(config: &Config, no_verify: bool) -> Result<()> {
    if !keychain::is_available() {
        bail!(
            "there is no keychain here — that is a macOS thing. Set ${} instead",
            config::API_KEY_ENV
        );
    }
    let key = read_key()?;
    if key.is_empty() {
        bail!("no key given");
    }

    if no_verify {
        eprintln!("not checking the key against the API.");
    } else {
        api::Client::new(key.clone())?
            .check_key()
            .await
            .context("the key was not stored — pass --no-verify to store it unchecked")?;
        eprintln!("Trafikverket accepts the key.");
    }

    keychain::set(&key)?;
    eprintln!(
        "stored in the keychain as {} for {}.",
        keychain::service(),
        keychain::ACCOUNT
    );

    if config.file_has_key() {
        eprintln!(
            "note: {} still has an api-key line. The keychain wins, but the file is \
             the copy that gets committed by accident — remove it.",
            config::config_path().display()
        );
    }
    if std::env::var_os(config::API_KEY_ENV).is_some() {
        eprintln!(
            "note: ${} is set in this shell, and it takes precedence over the keychain.",
            config::API_KEY_ENV
        );
    }
    Ok(())
}

/// Read the key without echoing it. A pipe works too, so the key can come
/// from a password manager rather than a paste.
fn read_key() -> Result<String> {
    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("could not read the key from stdin")?;
        return Ok(line.trim().to_string());
    }
    let key = dialoguer::Password::new()
        .with_prompt("Trafikverket API key")
        .interact()
        .context("could not read the key")?;
    Ok(key.trim().to_string())
}

fn auth_status(config: &Config) -> Result<()> {
    match config.api_key()? {
        Some((_, source)) => println!("in use: {}", source.describe()),
        None => println!("in use: no key — run `trafikverket auth`"),
    }
    println!(
        "${}: {}",
        config::API_KEY_ENV,
        if std::env::var_os(config::API_KEY_ENV).is_some() {
            "set"
        } else {
            "not set"
        }
    );
    println!(
        "keychain: {}",
        if !keychain::is_available() {
            "not available on this platform".to_string()
        } else if keychain::get()?.is_some() {
            format!("a key is stored as {}", keychain::service())
        } else {
            "nothing stored".to_string()
        }
    );
    println!(
        "{}: {}",
        config::config_path().display(),
        if config.file_has_key() {
            "has an api-key line"
        } else {
            "no api-key line"
        }
    );
    Ok(())
}

fn auth_forget() -> Result<()> {
    if keychain::delete()? {
        eprintln!("removed the key from the keychain.");
    } else {
        eprintln!("there was no key in the keychain.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn zone() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn a_time_still_to_come_is_today() {
        let now = time("2026-09-25T07:54:00+02:00");
        assert_eq!(
            next_occurrence(&zone(), now, at(16, 30)).unwrap(),
            time("2026-09-25T16:30:00+02:00")
        );
    }

    #[test]
    fn a_time_already_past_is_tomorrow() {
        let now = time("2026-09-25T17:00:00+02:00");
        assert_eq!(
            next_occurrence(&zone(), now, at(16, 30)).unwrap(),
            time("2026-09-26T16:30:00+02:00")
        );
    }

    #[test]
    fn the_current_minute_counts_as_past() {
        let now = time("2026-09-25T16:30:00+02:00");
        assert_eq!(
            next_occurrence(&zone(), now, at(16, 30)).unwrap(),
            time("2026-09-26T16:30:00+02:00")
        );
    }
}
