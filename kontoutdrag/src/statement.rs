//! Reading a bank statement export into transactions, and pulling the
//! merchant descriptor out of the one free-text field banks give us.

use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;

use crate::amount::Amount;

/// One posted transaction.
#[derive(Debug, Clone)]
pub struct Transaction {
    /// The day the transaction hit the account.
    pub booked: NaiveDate,
    /// The day it counts from for interest. Usually equal to `booked`.
    pub value_date: NaiveDate,
    /// The bank's posting-batch identifier. Shared by every transaction
    /// posted in the same run, so it is not a transaction id.
    pub batch: String,
    /// The `Text` field exactly as exported.
    pub text: String,
    pub amount: Amount,
    /// Account balance after the transaction, when the export carries one.
    pub balance: Option<Amount>,
    pub descriptor: Descriptor,
}

/// What the free-text field turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Descriptor {
    /// A card purchase: a truncated merchant name plus the date of the
    /// purchase itself, which is earlier than the posting date.
    Card {
        merchant: String,
        purchased: Option<NaiveDate>,
    },
    /// A Swish payment, identified only by the counterparty's number.
    Swish { number: String },
    /// Everything else: bankgiro and autogiro creditor names, transfers
    /// between own accounts, interest, salary.
    Plain { text: String },
}

impl Descriptor {
    /// The string to look a merchant up by.
    pub fn key(&self) -> &str {
        match self {
            Descriptor::Card { merchant, .. } => merchant,
            Descriptor::Swish { number } => number,
            Descriptor::Plain { text } => text,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Descriptor::Card { .. } => "card",
            Descriptor::Swish { .. } => "swish",
            Descriptor::Plain { .. } => "plain",
        }
    }
}

/// Split a statement `Text` field into its parts.
///
/// SEB writes card purchases as a merchant name padded to exactly twelve
/// characters, a slash, and the purchase date as `YY-MM-DD`. Nothing else
/// in the file has that shape, so the pattern is safe to key on. Pure
/// digits are a Swish counterparty: eleven digits is a phone number in
/// international form without the plus, ten is a Swish-företag number.
pub fn parse_descriptor(text: &str) -> Descriptor {
    if let Some(descriptor) = parse_card_descriptor(text) {
        return descriptor;
    }
    let trimmed = text.trim();
    if trimmed.len() >= 8 && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Descriptor::Swish {
            number: trimmed.to_string(),
        };
    }
    Descriptor::Plain {
        text: trimmed.to_string(),
    }
}

fn parse_card_descriptor(text: &str) -> Option<Descriptor> {
    // Char count, not byte length: a truncated name can end mid-word on a
    // character that is two bytes in UTF-8.
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != 21 || chars[12] != '/' {
        return None;
    }
    let merchant: String = chars[..12].iter().collect();
    let date: String = chars[13..].iter().collect();
    let purchased = NaiveDate::parse_from_str(&date, "%y-%m-%d").ok()?;
    Some(Descriptor::Card {
        merchant: merchant.trim().to_string(),
        purchased: Some(purchased),
    })
}

/// The column layout of a statement export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// SEB internetbanken's "Spara kontohändelser" CSV: semicolon
    /// separated, UTF-8 with a BOM, six columns.
    Seb,
}

impl std::str::FromStr for Format {
    type Err = anyhow::Error;
    fn from_str(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            "seb" => Ok(Format::Seb),
            other => bail!("unknown statement format {other:?} (known formats: seb)"),
        }
    }
}

pub fn read_file(path: &Path, format: Format) -> Result<Vec<Transaction>> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    parse(&contents, format).with_context(|| format!("in {}", path.display()))
}

pub fn parse(contents: &str, format: Format) -> Result<Vec<Transaction>> {
    match format {
        Format::Seb => parse_seb(contents),
    }
}

/// Columns SEB writes, in order. Matched by name so a future reordering or
/// an added column is tolerated, but an unrecognised header is an error
/// rather than a guess.
const SEB_COLUMNS: [&str; 6] = [
    "Bokföringsdatum",
    "Valutadatum",
    "Verifikationsnummer",
    "Text",
    "Belopp",
    "Saldo",
];

fn parse_seb(contents: &str) -> Result<Vec<Transaction>> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut lines = contents.lines().filter(|line| !line.trim().is_empty());

    let header = lines.next().context("the file is empty")?;
    let columns: Vec<&str> = header.split(';').map(str::trim).collect();
    for required in SEB_COLUMNS.iter().take(5) {
        if !columns.contains(required) {
            bail!(
                "this does not look like an SEB statement: the header has no {required:?} column \
                 (found {columns:?})"
            );
        }
    }
    let index = |name: &str| columns.iter().position(|c| c == &name);
    let (booked, value_date, batch, text, amount) = (
        index("Bokföringsdatum").unwrap(),
        index("Valutadatum").unwrap(),
        index("Verifikationsnummer").unwrap(),
        index("Text").unwrap(),
        index("Belopp").unwrap(),
    );
    let balance = index("Saldo");

    let mut transactions = Vec::new();
    for (offset, line) in lines.enumerate() {
        let number = offset + 2;
        let fields: Vec<&str> = line.split(';').collect();
        let field = |i: usize| -> Result<&str> {
            fields
                .get(i)
                .copied()
                .with_context(|| format!("line {number}: expected {} columns", columns.len()))
        };

        let date = |i: usize| -> Result<NaiveDate> {
            let raw = field(i)?.trim();
            NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .with_context(|| format!("line {number}: {raw:?} is not an ISO date"))
        };

        let text_value = field(text)?.trim().to_string();
        let amount_value = field(amount)?
            .trim()
            .parse::<Amount>()
            .with_context(|| format!("line {number}: bad Belopp"))?;
        let balance_value = match balance {
            Some(i) => field(i)?.trim().parse::<Amount>().ok(),
            None => None,
        };

        transactions.push(Transaction {
            booked: date(booked)?,
            value_date: date(value_date)?,
            batch: field(batch)?.trim().to_string(),
            descriptor: parse_descriptor(&text_value),
            text: text_value,
            amount: amount_value,
            balance: balance_value,
        });
    }
    Ok(transactions)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}Bokföringsdatum;Valutadatum;Verifikationsnummer;Text;Belopp;Saldo\n\
        2026-09-11;2026-09-11;1000000001;PRESSHORNAN /26-09-10;-30.000;10000.000\n\
        2026-09-11;2026-09-11;1000000002;46700000001;-200.000;10030.000\n\
        2026-09-10;2026-09-10;1000000003;VTABERGS TANDLAKARE AB;-800.000;10230.000\n";

    #[test]
    fn reads_the_seb_export() {
        let transactions = parse(SAMPLE, Format::Seb).unwrap();
        assert_eq!(transactions.len(), 3);

        let first = &transactions[0];
        assert_eq!(first.booked, NaiveDate::from_ymd_opt(2026, 9, 11).unwrap());
        assert_eq!(first.amount, "-30".parse().unwrap());
        assert_eq!(
            first.descriptor,
            Descriptor::Card {
                merchant: "PRESSHORNAN".into(),
                purchased: NaiveDate::from_ymd_opt(2026, 9, 10),
            }
        );
    }

    #[test]
    fn tells_the_three_descriptor_shapes_apart() {
        assert_eq!(
            parse_descriptor("KVARNBY LIVS/26-09-10"),
            Descriptor::Card {
                merchant: "KVARNBY LIVS".into(),
                purchased: NaiveDate::from_ymd_opt(2026, 9, 10),
            }
        );
        assert_eq!(
            parse_descriptor("VTABERGET   /26-09-08"),
            Descriptor::Card {
                merchant: "VTABERGET".into(),
                purchased: NaiveDate::from_ymd_opt(2026, 9, 8),
            }
        );
        assert_eq!(
            parse_descriptor("46700000001"),
            Descriptor::Swish {
                number: "46700000001".into()
            }
        );
        assert_eq!(
            parse_descriptor("KVARNBY BOSTADSFÖRENING"),
            Descriptor::Plain {
                text: "KVARNBY BOSTADSFÖRENING".into()
            }
        );
    }

    /// A name truncated to twelve characters can end on a multi-byte one.
    #[test]
    fn counts_characters_not_bytes() {
        assert_eq!(
            parse_descriptor("KÖPMANS TORÅ/26-09-10"),
            Descriptor::Card {
                merchant: "KÖPMANS TORÅ".into(),
                purchased: NaiveDate::from_ymd_opt(2026, 9, 10),
            }
        );
    }

    /// A loan number is digits but not a Swish number, and a merchant name
    /// that merely contains digits is not either.
    #[test]
    fn does_not_mistake_short_digit_strings_for_swish() {
        assert!(matches!(
            parse_descriptor("1234567"),
            Descriptor::Plain { .. }
        ));
        assert!(matches!(
            parse_descriptor("LÅN 10000001"),
            Descriptor::Plain { .. }
        ));
    }

    #[test]
    fn rejects_a_file_that_is_not_a_statement() {
        let error = parse("name,value\nfoo,1\n", Format::Seb).unwrap_err();
        assert!(error.to_string().contains("does not look like"));
    }
}
