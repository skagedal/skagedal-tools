//! The subcommands, and the loading both of them share.

pub mod edit_config;
pub mod explain;
pub mod list;
pub mod marks;
pub mod summary;
pub mod tables;
pub mod unmatched;

use anyhow::{Context, Result};
use chrono::NaiveDate;

use crate::cli::Common;
use crate::config::{self, Settings};
use crate::mapping::{self, Table};
use crate::marks::{Marked, Marks};
use crate::matcher::{Hit, Matcher};
use crate::statement::{self, Transaction};

/// Load the configured tables, plus any given with `--table`.
pub fn load_matcher(extra: &[std::path::PathBuf], only_extra: bool) -> Result<Matcher> {
    let settings = config::load(&crate::paths::config_path())?;
    load_matcher_with(&settings, extra, only_extra)
}

pub fn load_matcher_with(
    settings: &Settings,
    extra: &[std::path::PathBuf],
    only_extra: bool,
) -> Result<Matcher> {
    let mut tables: Vec<Table> = if only_extra {
        Vec::new()
    } else {
        settings.load_tables()?
    };
    for path in extra {
        tables.push(mapping::load_path(path)?);
    }
    if tables.is_empty() {
        anyhow::bail!("--only-tables was given without any --table");
    }
    Matcher::build(&tables)
}

/// A transaction together with what the tables made of it, and then what
/// any hand-written marks made of that.
pub struct Resolved {
    pub transaction: Transaction,
    pub hit: Option<Hit>,
    pub marked: Marked,
}

impl Resolved {
    /// A mark naming the merchant wins over the tables, because a mark is
    /// written about one transaction and a rule about a whole descriptor.
    pub fn merchant(&self) -> &str {
        self.marked
            .merchant
            .as_deref()
            .or_else(|| self.hit.as_ref().map(|h| h.name.as_str()))
            .unwrap_or("")
    }

    pub fn category(&self) -> &str {
        self.marked
            .category
            .as_deref()
            .or_else(|| self.hit.as_ref().and_then(|h| h.category.as_deref()))
            .unwrap_or("")
    }

    /// Tags from the tables and from the marks, in that order, without
    /// repeats. Marks add to tags rather than replacing them.
    pub fn tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .hit
            .as_ref()
            .map(|h| h.tags.clone())
            .unwrap_or_default();
        for tag in &self.marked.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
        tags
    }

    /// Whether anything at all named this transaction.
    pub fn is_resolved(&self) -> bool {
        self.hit.is_some() || self.marked.merchant.is_some() || self.marked.category.is_some()
    }
}

/// Everything the reporting commands need: the statement, resolved, and
/// filtered down to the requested window.
pub struct Loaded {
    pub transactions: Vec<Resolved>,
    /// How many rows the date and direction filters removed.
    pub filtered_out: usize,
    /// The marks that were loaded, and how many rows each one caught.
    pub marks: Marks,
    pub mark_hits: Vec<usize>,
}

pub fn load(common: &Common) -> Result<Loaded> {
    let settings = config::load(&crate::paths::config_path())?;
    let matcher = load_matcher_with(&settings, &common.tables, common.only_tables)?;

    let format = match &common.format {
        Some(name) => Some(name.parse()?),
        None => None,
    };
    let path = settings.resolve_statement(&common.statement)?;
    let transactions = statement::read_file(&path, format, settings.format()?)?;
    let total = transactions.len();

    let from = parse_date(common.from.as_deref(), "--from")?;
    let to = parse_date(common.to.as_deref(), "--to")?;

    let marks = Marks::build(&settings.load_marks()?);
    let mut mark_hits = vec![0usize; marks.len()];

    let resolved: Vec<Resolved> = transactions
        .into_iter()
        .filter(|t| from.is_none_or(|d| t.booked >= d) && to.is_none_or(|d| t.booked <= d))
        .filter(|t| {
            if common.spending {
                t.amount.is_negative()
            } else if common.income {
                !t.amount.is_negative()
            } else {
                true
            }
        })
        .map(|transaction| {
            let hit = matcher.lookup(transaction.descriptor.key());
            let marked = marks.apply(&transaction);
            for index in &marked.applied {
                mark_hits[*index] += 1;
            }
            Resolved {
                transaction,
                hit,
                marked,
            }
        })
        .collect();

    Ok(Loaded {
        filtered_out: total - resolved.len(),
        transactions: resolved,
        marks,
        mark_hits,
    })
}

fn parse_date(value: Option<&str>, flag: &str) -> Result<Option<NaiveDate>> {
    value
        .map(|raw| {
            NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .with_context(|| format!("{flag}: {raw:?} is not a date of the form YYYY-MM-DD"))
        })
        .transpose()
}
