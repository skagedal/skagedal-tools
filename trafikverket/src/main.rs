//! `trafikverket` — the next trains between two stations, and how late they
//! are, from Trafikverket's open API.

use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use chrono::Local;
use clap::Parser;

mod api;
mod cli;
mod config;
mod journeys;
mod model;
mod output;
mod query;
mod stations;
mod ticket;

use cli::{Cli, Command, ConfigAction, ConfigArgs, NextArgs, RawArgs, StationsArgs};
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

    let (departures, arrivals) = tokio::try_join!(
        client.announcements(
            &from.signature,
            query::DEPARTURE,
            -LOOKBACK_MINUTES,
            args.window,
        ),
        client.announcements(
            &to.signature,
            query::ARRIVAL,
            -LOOKBACK_MINUTES,
            args.window + ARRIVAL_ALLOWANCE_MINUTES,
        ),
    )?;

    let now = Local::now().fixed_offset();
    let selection = journeys::select(
        journeys::build(&departures, &arrivals, &ticket),
        now,
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
            println!(
                "api key: {}",
                if config.api_key().is_some() {
                    "set"
                } else {
                    "missing"
                }
            );
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
    config.api_key().with_context(|| {
        format!(
            "no API key. Get one free from Trafikverket's data portal at \
             https://data.trafikverket.se, then set ${} or put `api-key = \"…\"` in {}",
            config::API_KEY_ENV,
            config::config_path().display()
        )
    })
}
