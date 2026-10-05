//! Read the Messages app's history from the terminal: every conversation in
//! a span of days, or the ones with one person.
//!
//! The Messages and Contacts databases are protected by Full Disk Access,
//! so this has to run from a terminal that has it. Both are opened
//! read-only.

mod contacts;
mod messages;

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{Duration, Local, NaiveDate, TimeZone};
use clap::Parser;

use contacts::Contacts;
use messages::Message;

#[derive(Parser)]
#[command(
    name = "imessage-log",
    version,
    about = "Print Messages history for a span of days, optionally for one contact",
    after_help = "Needs Full Disk Access: ~/Library/Messages is protected, and so is Contacts."
)]
struct Cli {
    /// First day, YYYY-MM-DD. Defaults to a week before --to.
    #[arg(long)]
    from: Option<NaiveDate>,

    /// Last day, YYYY-MM-DD, included. Defaults to today.
    #[arg(long)]
    to: Option<NaiveDate>,

    /// Only conversations with someone whose name, number or address
    /// contains this. Group chats they are in count, unless --direct.
    #[arg(long, short)]
    contact: Option<String>,

    /// With --contact, leave out group chats.
    #[arg(long, requires = "contact")]
    direct: bool,

    /// Only messages containing this, in any case.
    #[arg(long, short)]
    text: Option<String>,

    /// The Messages database.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    /// The Contacts directory, holding AddressBook-v22.abcddb.
    #[arg(long, value_name = "DIR")]
    address_book: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("imessage-log: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
    let db_path = cli
        .db
        .unwrap_or_else(|| home.join("Library/Messages/chat.db"));
    let address_book = cli
        .address_book
        .unwrap_or_else(|| home.join("Library/Application Support/AddressBook"));

    let to = cli.to.unwrap_or_else(|| Local::now().date_naive());
    let from = cli.from.unwrap_or(to - Duration::days(7));
    if from > to {
        bail!("--from {from} is after --to {to}");
    }
    let start = local_midnight(from)?;
    let end = local_midnight(to + Duration::days(1))?;

    let db = messages::open(&db_path)?;
    let all = messages::read(&db, start, end)?;
    let contacts = Contacts::load(&address_book).unwrap_or_else(|e| {
        eprintln!("imessage-log: showing numbers, not names — could not read Contacts: {e:#}");
        Contacts::default()
    });
    if contacts.is_empty() {
        eprintln!("imessage-log: no names found in Contacts, so numbers are shown as they are");
    }

    let contact = cli.contact.as_deref().map(str::to_lowercase);
    let text = cli.text.as_deref().map(str::to_lowercase);
    let shown: Vec<&Message> = all
        .iter()
        .filter(|m| match &contact {
            Some(c) => (!cli.direct || !m.group) && involves(m, c, &contacts),
            None => true,
        })
        .filter(|m| {
            text.as_ref()
                .is_none_or(|t| m.text.to_lowercase().contains(t))
        })
        .collect();

    let mut out = io::stdout().lock();
    print(&mut out, &shown, &contacts)?;
    if shown.is_empty() {
        eprintln!("imessage-log: no messages from {from} to {to} matched");
    }
    Ok(())
}

fn local_midnight(day: NaiveDate) -> Result<chrono::DateTime<chrono::Utc>> {
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.with_timezone(&chrono::Utc))
        .with_context(|| format!("{day} has no local midnight"))
}

/// Whether the conversation has someone in it matching `query`, by name or
/// by number. Digits are compared normalised, so `070 000 00 01` finds
/// `+46700000001`.
fn involves(m: &Message, query: &str, contacts: &Contacts) -> bool {
    let digits: String = query.chars().filter(char::is_ascii_digit).collect();
    let wanted = (digits.len() >= 4).then(|| contacts::normalize(query));
    m.participants
        .iter()
        .chain(std::iter::once(&m.sender).filter(|s| !s.is_empty()))
        .any(|handle| {
            contacts.name(handle).to_lowercase().contains(query)
                || handle.to_lowercase().contains(query)
                || wanted
                    .as_ref()
                    .is_some_and(|w| contacts::normalize(handle).contains(w.as_str()))
        })
}

fn print(out: &mut impl Write, shown: &[&Message], contacts: &Contacts) -> io::Result<()> {
    let mut day = None;
    for m in shown {
        let date = m.sent.date_naive();
        if day != Some(date) {
            if day.is_some() {
                writeln!(out)?;
            }
            writeln!(out, "── {}", m.sent.format("%Y-%m-%d, %A"))?;
            day = Some(date);
        }
        let names: Vec<&str> = m.participants.iter().map(|h| contacts.name(h)).collect();
        let who = if m.group {
            let chat = m.chat_name.clone().unwrap_or_else(|| names.join(", "));
            let sender = if m.from_me {
                "me"
            } else {
                contacts.name(&m.sender)
            };
            format!("[{chat}] {sender}")
        } else if m.from_me {
            format!("me → {}", names.join(", "))
        } else {
            contacts.name(&m.sender).to_string()
        };
        let mut text = m.text.replace('\n', "\n       ");
        if m.attachments {
            text = if text.is_empty() {
                "[attachment]".to_string()
            } else {
                format!("{text} [+attachment]")
            };
        }
        writeln!(out, "{}  {who}: {text}", m.sent.format("%H:%M"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> Vec<Message> {
        let (_dir, path) = messages::tests::fixture();
        let db = messages::open(&path).unwrap();
        let from = local_midnight(NaiveDate::from_ymd_opt(2026, 4, 10).unwrap()).unwrap();
        messages::read(&db, from, from + chrono::Duration::days(1)).unwrap()
    }

    fn render(shown: &[&Message]) -> String {
        let mut out = Vec::new();
        print(&mut out, shown, &Contacts::default()).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn prints_a_day_of_conversations() {
        let all = day();
        let shown: Vec<&Message> = all.iter().collect();
        assert_eq!(
            render(&shown),
            "── 2026-04-10, Friday\n\
             19:00  +46700000001: Where shall we eat?\n\
             19:05  me → +46700000001: The usual place\n\
             20:00  [Book club] someone@example.com: [attachment]\n\
             23:00  [Book club] someone@example.com: Next day almost\n"
        );
    }

    #[test]
    fn a_contact_is_found_by_number_however_written() {
        let all = day();
        let contacts = Contacts::default();
        let direct: Vec<&Message> = all
            .iter()
            .filter(|m| !m.group && involves(m, "070-000 00 01", &contacts))
            .collect();
        assert_eq!(direct.len(), 2);
        let with_groups = all
            .iter()
            .filter(|m| involves(m, "070 000 00 01", &contacts))
            .count();
        assert_eq!(with_groups, 4);
        assert!(!all.iter().any(|m| involves(m, "nobody", &contacts)));
    }
}
