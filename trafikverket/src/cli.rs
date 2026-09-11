//! Command line surface.

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "trafikverket",
    version,
    about = "The next trains between two stations, with live delays, from Trafikverket's open API."
)]
pub struct Cli {
    #[command(flatten)]
    pub next: NextArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List station signatures, optionally filtered by name
    Stations(StationsArgs),
    /// Show or edit the configuration file
    Config(ConfigArgs),
    /// Post a query to the API and print the reply, for finding out what the
    /// API really calls things
    Raw(RawArgs),
}

#[derive(Debug, Args)]
pub struct NextArgs {
    /// Route from the configuration file (default: the one named by default-route)
    #[arg(long, value_name = "NAME", conflicts_with_all = ["from", "to"])]
    pub route: Option<String>,

    /// Origin: a signature such as U, or a name such as "Uppsala C"
    #[arg(long, value_name = "STATION", requires = "to")]
    pub from: Option<String>,

    /// Destination: a signature such as Cst, or a name such as "Stockholm C"
    #[arg(long, value_name = "STATION", requires = "from")]
    pub to: Option<String>,

    /// Travel the other way
    #[arg(long, short = 'r')]
    pub reverse: bool,

    /// A train product the ticket covers; repeat for several (overrides the route's list)
    #[arg(long = "product", value_name = "NAME")]
    pub products: Vec<String>,

    /// Report every train, whatever its product
    #[arg(long, conflicts_with = "products")]
    pub any_product: bool,

    /// How many departures to show
    #[arg(long, short = 'n', value_name = "N", default_value_t = 3,
          value_parser = clap::value_parser!(u32).range(1..=50))]
    pub count: u32,

    /// How far ahead to look: 45m, 3h, 1h30m
    #[arg(long, short = 'w', value_name = "DURATION", default_value = "3h",
          value_parser = parse_window)]
    pub window: i64,

    /// Include cancelled departures, and ones the ticket does not cover
    #[arg(long, short = 'a')]
    pub all: bool,

    /// Print JSON instead of a table
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
#[command(group = clap::ArgGroup::new("what").required(true))]
pub struct RawArgs {
    /// Object type to dump every field of, e.g. TrainAnnouncement
    #[arg(long, value_name = "TYPE", group = "what")]
    pub object: Option<String>,

    /// Read the <QUERY> document from a file, or from stdin with -
    #[arg(long, value_name = "FILE", group = "what")]
    pub query: Option<String>,

    /// Schema version (defaults to the one this tool uses for known types)
    #[arg(long, value_name = "VERSION", conflicts_with = "query")]
    pub schema: Option<String>,

    /// Ask for at most this many rows
    #[arg(long, value_name = "N", conflicts_with = "query")]
    pub limit: Option<u32>,

    /// Filter elements to put inside <FILTER>, as XML
    #[arg(long, value_name = "XML", conflicts_with = "query")]
    pub filter: Option<String>,

    /// Pretty-print the JSON reply
    #[arg(long, short = 'p')]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct StationsArgs {
    /// Show only stations whose name or signature matches
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    /// Fetch the station list again instead of using the cached copy
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: Option<ConfigAction>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the path to the configuration file
    Path,
    /// Open the configuration file in $VISUAL or $EDITOR, creating it if needed
    Edit,
}

/// The longest window worth asking the API for. Beyond a day the timetable
/// answer stops being about the trip you are taking now.
const MAX_WINDOW_MINUTES: i64 = 24 * 60;

/// Parse a look-ahead window: a bare number of minutes, or a combination of
/// hours and minutes such as `3h`, `45m` or `1h30m`.
fn parse_window(value: &str) -> Result<i64, String> {
    let text: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if text.is_empty() {
        return Err("empty duration".to_string());
    }

    let mut minutes: i64 = 0;
    let mut digits = String::new();
    let mut saw_unit = false;
    for ch in text.chars() {
        match ch {
            '0'..='9' => digits.push(ch),
            'h' | 'H' | 'm' | 'M' => {
                let number: i64 = digits
                    .parse()
                    .map_err(|_| format!("expected a number before '{ch}' in {value:?}"))?;
                digits.clear();
                saw_unit = true;
                minutes += if ch.eq_ignore_ascii_case(&'h') {
                    number
                        .checked_mul(60)
                        .ok_or_else(|| format!("{value:?} is too long"))?
                } else {
                    number
                };
            }
            _ => return Err(format!("unexpected '{ch}' in {value:?}")),
        }
    }
    if !digits.is_empty() {
        if saw_unit {
            return Err(format!("missing a unit at the end of {value:?}"));
        }
        // A bare number is minutes.
        minutes = digits
            .parse()
            .map_err(|_| format!("{value:?} is too long"))?;
    } else if !saw_unit {
        return Err(format!("no duration in {value:?}"));
    }

    if minutes < 1 {
        return Err("the window must be at least a minute".to_string());
    }
    if minutes > MAX_WINDOW_MINUTES {
        return Err("the window must be at most 24h".to_string());
    }
    Ok(minutes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_window("45"), Ok(45));
        assert_eq!(parse_window("45m"), Ok(45));
        assert_eq!(parse_window("3h"), Ok(180));
        assert_eq!(parse_window("1h30m"), Ok(90));
        assert_eq!(parse_window("1h 30m"), Ok(90));
        assert_eq!(parse_window("2H"), Ok(120));
    }

    #[test]
    fn rejects_nonsense_durations() {
        for bad in ["", "   ", "h", "3d", "1h30", "-5", "abc", "0", "0m", "25h"] {
            assert!(parse_window(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn defaults_are_a_three_hour_window_and_three_departures() {
        let cli = Cli::try_parse_from(["trafikverket"]).unwrap();
        assert_eq!(cli.next.window, 180);
        assert_eq!(cli.next.count, 3);
        assert!(!cli.next.all);
    }

    #[test]
    fn from_and_to_come_as_a_pair() {
        assert!(Cli::try_parse_from(["trafikverket", "--from", "U"]).is_err());
        assert!(Cli::try_parse_from(["trafikverket", "--to", "Cst"]).is_err());
        assert!(Cli::try_parse_from(["trafikverket", "--from", "U", "--to", "Cst"]).is_ok());
    }

    #[test]
    fn a_named_route_and_an_ad_hoc_one_are_exclusive() {
        assert!(
            Cli::try_parse_from([
                "trafikverket",
                "--route",
                "commute",
                "--from",
                "U",
                "--to",
                "Cst"
            ])
            .is_err()
        );
    }

    #[test]
    fn products_can_be_repeated() {
        let cli = Cli::try_parse_from([
            "trafikverket",
            "--product",
            "Mälartåg",
            "--product",
            "SJ Regional",
        ])
        .unwrap();
        assert_eq!(cli.next.products, vec!["Mälartåg", "SJ Regional"]);
    }

    #[test]
    fn raw_needs_something_to_ask_for() {
        assert!(Cli::try_parse_from(["trafikverket", "raw"]).is_err());
        assert!(Cli::try_parse_from(["trafikverket", "raw", "--object", "TrainStation"]).is_ok());
        assert!(Cli::try_parse_from(["trafikverket", "raw", "--query", "q.xml"]).is_ok());
    }

    #[test]
    fn raw_cannot_mix_a_document_with_a_built_query() {
        assert!(
            Cli::try_parse_from([
                "trafikverket",
                "raw",
                "--query",
                "q.xml",
                "--object",
                "TrainStation"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["trafikverket", "raw", "--query", "q.xml", "--limit", "1"])
                .is_err()
        );
    }

    #[test]
    fn subcommands_parse() {
        let cli = Cli::try_parse_from(["trafikverket", "stations", "uppsala"]).unwrap();
        match cli.command {
            Some(Command::Stations(args)) => assert_eq!(args.query.as_deref(), Some("uppsala")),
            other => panic!("expected stations, got {other:?}"),
        }
        let cli = Cli::try_parse_from(["trafikverket", "config", "edit"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigArgs {
                action: Some(ConfigAction::Edit)
            }))
        ));
    }
}
