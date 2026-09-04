use clap::{Args, Parser, Subcommand as ClapSubcommand};

const TIME_DESCRIPTION: &str = "time range. Examples: 5h, 30m, 500ms, 13.00-13.01, \
     09:15:00.000-09:15:00.500, \"yesterday 17-18\", \
     2026-04-22T13:00:00Z/2026-04-22T14:00:00Z. \
     Defaults to the front-matter `time` if present, otherwise 1h.";

#[derive(Debug, Parser)]
#[command(
    name = "cloudwatch-insights",
    version,
    about = "Download logs from AWS CloudWatch Logs Insights."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Subcommand,
}

#[derive(Debug, ClapSubcommand)]
pub enum Subcommand {
    /// Run a CloudWatch Logs Insights query (opens $EDITOR when no -q/-f is given)
    Query(QueryArgs),
    /// Run a query verbatim from a file (no templating, no front-matter, no config)
    Raw(RawArgs),
    /// Copy a shareable AWS Console URL for the query to the system pasteboard
    #[command(name = "copy-link")]
    CopyLink(CopyLinkArgs),
    /// Decode an AWS Console Insights URL (default: from the pasteboard) and recreate the query state
    #[command(name = "paste-link")]
    PasteLink(PasteLinkArgs),
    /// Stream the contents of latest-run.jsonl to stdout
    Show(ShowArgs),
    /// Open settings.toml in $EDITOR, creating it (and a placeholder section for the current repo) if needed
    #[command(name = "edit-config")]
    EditConfig,
}

#[derive(Debug, Args, Clone)]
pub struct QueryArgs {
    /// Log group name (repeat or comma-separate; overrides config)
    #[arg(long = "log-group", short = 'g', value_delimiter = ',')]
    pub log_group: Vec<String>,

    /// Time range
    #[arg(long, short = 't', help = TIME_DESCRIPTION)]
    pub time: Option<String>,

    /// CloudWatch Insights query string (skips editor)
    #[arg(long, short = 'q')]
    pub query: Option<String>,

    /// Read query from file (use '-' for stdin; skips editor)
    #[arg(long = "query-file", short = 'f')]
    pub query_file: Option<String>,

    /// Environment name (lower kebab-case); substituted for {{ env }} in the query and log group template
    #[arg(long, short = 'e')]
    pub environment: Option<String>,

    /// Maximum number of rows to return
    #[arg(long, short = 'l')]
    pub limit: Option<i32>,

    /// AWS region (overrides AWS_REGION)
    #[arg(long, short = 'r')]
    pub region: Option<String>,

    /// AWS profile (sets AWS_PROFILE)
    #[arg(long)]
    pub profile: Option<String>,

    /// Reset current.insights to the default template before opening the editor
    #[arg(long, default_value_t = false)]
    pub new: bool,

    /// Suppress progress output on stderr
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
}

#[derive(Debug, Args, Clone)]
pub struct RawArgs {
    /// Log group name (repeat or comma-separate; required)
    #[arg(long = "log-group", short = 'g', value_delimiter = ',')]
    pub log_group: Vec<String>,

    /// Read query from file (use '-' for stdin; required)
    #[arg(long = "query-file", short = 'f')]
    pub query_file: Option<String>,

    /// Time range (same syntax as `query --time`). Defaults to 1h.
    #[arg(long, short = 't')]
    pub time: Option<String>,

    /// Maximum number of rows to return
    #[arg(long, short = 'l')]
    pub limit: Option<i32>,

    /// AWS region (overrides AWS_REGION)
    #[arg(long, short = 'r')]
    pub region: Option<String>,

    /// AWS profile (sets AWS_PROFILE)
    #[arg(long)]
    pub profile: Option<String>,

    /// Write JSONL results to this file instead of stdout
    #[arg(long, short = 'o')]
    pub output: Option<String>,

    /// Suppress progress output on stderr
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
}

#[derive(Debug, Args, Clone)]
pub struct CopyLinkArgs {
    /// Log group name (repeat or comma-separate; overrides config)
    #[arg(long = "log-group", short = 'g', value_delimiter = ',')]
    pub log_group: Vec<String>,

    /// Time range
    #[arg(long, short = 't', help = TIME_DESCRIPTION)]
    pub time: Option<String>,

    /// CloudWatch Insights query string (skips current.insights)
    #[arg(long, short = 'q')]
    pub query: Option<String>,

    /// Read query from file (use '-' for stdin; skips current.insights)
    #[arg(long = "query-file", short = 'f')]
    pub query_file: Option<String>,

    /// Environment name (lower kebab-case); substituted for {{ env }} in the query and log group template
    #[arg(long, short = 'e')]
    pub environment: Option<String>,

    /// AWS region (overrides AWS_REGION)
    #[arg(long, short = 'r')]
    pub region: Option<String>,

    /// Always emit absolute start/end timestamps in the URL, even when -t is a relative duration like 1h
    #[arg(long = "preserve-time-window", default_value_t = false)]
    pub preserve_time_window: bool,

    /// Print the URL to stdout and skip the pasteboard copy
    #[arg(long, default_value_t = false)]
    pub raw: bool,

    /// Open the URL in the default browser
    #[arg(long, default_value_t = false)]
    pub open: bool,

    /// Suppress diagnostic output on stderr
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
}

#[derive(Debug, Args, Clone)]
pub struct PasteLinkArgs {
    /// Print a self-contained `cloudwatch-insights raw` shell command instead of writing current.insights
    #[arg(long = "as-raw", default_value_t = false)]
    pub as_raw: bool,

    /// Write the .insights file to this path instead of the current slot's current.insights
    #[arg(long, short = 'o')]
    pub output: Option<String>,

    /// Prompt for the URL on stdin instead of reading the pasteboard
    #[arg(long, short = 'p', default_value_t = false)]
    pub prompt: bool,

    /// Suppress diagnostic output on stderr
    #[arg(long, default_value_t = false)]
    pub quiet: bool,

    /// Positional URL argument (overrides the pasteboard read)
    #[arg(value_name = "URL")]
    pub positional_url: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct ShowArgs {
    /// Print the path to latest-run.jsonl instead of its contents
    #[arg(long, default_value_t = false)]
    pub path: bool,
}
