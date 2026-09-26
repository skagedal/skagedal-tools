use std::path::PathBuf;

use clap::{Args, Parser, Subcommand as ClapSubcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "kontoutdrag",
    version,
    about = "Identify merchants and categorise spending in a bank statement export."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Subcommand,
}

#[derive(Debug, ClapSubcommand)]
pub enum Subcommand {
    /// Print every transaction with the merchant and category it resolved to
    List(ListArgs),
    /// Show the descriptors no table matched, busiest first — the work list
    Unmatched(UnmatchedArgs),
    /// Total spending per category, merchant or month
    Summary(SummaryArgs),
    /// Inspect the loaded merchant tables
    Tables(TablesArgs),
    /// Show the hand-written marks and how many rows each one caught
    Marks(MarksArgs),
    /// Look a single descriptor up and show which rule decided it
    Explain(ExplainArgs),
    /// Open the statements in a window with charts to click through
    View(ViewArgs),
    /// Open settings.toml in $EDITOR, creating it from the template if needed
    #[command(name = "edit-config")]
    EditConfig,
}

#[derive(Debug, Args)]
pub struct MarksArgs {
    #[command(flatten)]
    pub common: Common,

    #[arg(long, short = 'o', value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,
}

#[derive(Debug, Args, Clone)]
pub struct Common {
    /// Statement file to read
    pub statement: PathBuf,

    /// Statement format (overrides the config)
    #[arg(long, global = true)]
    pub format: Option<String>,

    /// Extra merchant table, after the configured ones (repeatable)
    #[arg(long = "table", short = 't', global = true)]
    pub tables: Vec<PathBuf>,

    /// Ignore the configured tables and use only those given with --table
    #[arg(long, global = true)]
    pub only_tables: bool,

    /// Keep only transactions booked on or after this date (YYYY-MM-DD)
    #[arg(long, global = true)]
    pub from: Option<String>,

    /// Keep only transactions booked on or before this date (YYYY-MM-DD)
    #[arg(long, global = true)]
    pub to: Option<String>,

    /// Keep only money out (default is everything)
    #[arg(long, global = true, conflicts_with = "income")]
    pub spending: bool,

    /// Keep only money in
    #[arg(long, global = true, conflicts_with = "spending")]
    pub income: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Aligned columns for reading
    #[default]
    Table,
    /// Tab-separated, for piping into something else
    Tsv,
    /// One JSON object per line
    Json,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    pub common: Common,

    /// Show only transactions that resolved to this merchant
    #[arg(long, short = 'm')]
    pub merchant: Option<String>,

    /// Show only transactions in this category
    #[arg(long, short = 'c')]
    pub category: Option<String>,

    /// Show only transactions no table matched
    #[arg(long)]
    pub unresolved: bool,

    /// Add a column naming the table and rule that decided each merchant
    #[arg(long)]
    pub explain: bool,

    /// Add the columns the statement carries but the default view drops:
    /// value date, posting batch, running balance and the raw Text field
    #[arg(long)]
    pub full: bool,

    #[arg(long, short = 'o', value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct UnmatchedArgs {
    #[command(flatten)]
    pub common: Common,

    /// Show at most this many descriptors
    #[arg(long, short = 'n')]
    pub limit: Option<usize>,

    /// Show only descriptors seen at least this many times
    #[arg(long, default_value_t = 1)]
    pub min_count: usize,

    /// Print a YAML stub for each descriptor, ready to paste into a table
    #[arg(long)]
    pub yaml: bool,

    #[arg(long, short = 'o', value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum GroupBy {
    #[default]
    Category,
    Merchant,
    Month,
    Tag,
}

#[derive(Debug, Args)]
pub struct SummaryArgs {
    #[command(flatten)]
    pub common: Common,

    /// What to total by
    #[arg(long, short = 'b', value_enum, default_value_t)]
    pub by: GroupBy,

    /// Show at most this many rows
    #[arg(long, short = 'n')]
    pub limit: Option<usize>,

    #[arg(long, short = 'o', value_enum, default_value_t)]
    pub output: OutputFormat,
}

#[derive(Debug, Args)]
pub struct TablesArgs {
    /// Extra merchant table, after the configured ones (repeatable)
    #[arg(long = "table", short = 't')]
    pub tables: Vec<PathBuf>,

    /// Ignore the configured tables and use only those given with --table
    #[arg(long)]
    pub only_tables: bool,

    /// List the tables compiled into this binary instead of the configured ones
    #[arg(long)]
    pub bundled: bool,

    /// Print a bundled table's YAML, to start your own from
    #[arg(long, value_name = "NAME")]
    pub dump: Option<String>,

    /// List every merchant across the loaded tables
    #[arg(long)]
    pub merchants: bool,
}

#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// The descriptor to look up, as it appears in the statement
    pub descriptor: String,

    /// Extra merchant table, after the configured ones (repeatable)
    #[arg(long = "table", short = 't')]
    pub tables: Vec<PathBuf>,

    /// Ignore the configured tables and use only those given with --table
    #[arg(long)]
    pub only_tables: bool,
}

#[derive(Debug, Args)]
pub struct ViewArgs {
    /// Statement files to read; several accounts are shown side by side
    #[arg(required = true)]
    pub statements: Vec<PathBuf>,

    /// Statement format (overrides the config)
    #[arg(long)]
    pub format: Option<String>,

    /// Extra merchant table, after the configured ones (repeatable)
    #[arg(long = "table", short = 't')]
    pub tables: Vec<PathBuf>,

    /// Ignore the configured tables and use only those given with --table
    #[arg(long)]
    pub only_tables: bool,

    /// Serve the view and print its URL instead of opening a window, for
    /// looking at it in an ordinary browser
    #[arg(long, conflicts_with = "json")]
    pub serve: bool,

    /// Print the data the view is drawn from, as JSON, and exit
    #[arg(long)]
    pub json: bool,
}
