//! Reading a bank statement export into transactions, and pulling the
//! merchant descriptor out of the one free-text field banks give us.

use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::Deserialize;

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
    /// The bank's own identifier for this one transaction, when the export
    /// carries one. The CSV does not; Enable Banking's `entry_reference` is.
    pub reference: Option<String>,
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
    /// Enable Banking's account-information API: a JSON array of transactions, or the
    /// `{"transactions": [...]}` object the API itself returns.
    ///
    /// Worth having because it carries the merchant name the CSV does not.
    /// The CSV truncates a card descriptor to twelve characters, which for a
    /// foreign purchase can leave the acquirer's city and nothing else.
    EnableBanking,
}

impl std::str::FromStr for Format {
    type Err = anyhow::Error;
    fn from_str(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().replace('_', "-").as_str() {
            "seb" => Ok(Format::Seb),
            "enable-banking" | "enablebanking" => Ok(Format::EnableBanking),
            other => bail!(
                "unknown statement format {other:?} \
                 (known formats: seb, enable-banking)"
            ),
        }
    }
}

/// Read a statement.
///
/// `format` is the one explicitly asked for. `None` lets the file decide:
/// JSON is recognised by its opening bracket and read as Enable Banking,
/// and anything else falls through to `fallback`, which is whatever the
/// configuration says. Sniffing cannot misfire — a CSV export starts with
/// a byte-order mark or a column name, never a bracket.
pub fn read_file(
    path: &Path,
    format: Option<Format>,
    fallback: Format,
) -> Result<Vec<Transaction>> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let format = format.or_else(|| sniff(&contents)).unwrap_or(fallback);
    parse(&contents, format).with_context(|| format!("in {}", path.display()))
}

/// The format a file announces by its first non-blank character, or `None`
/// when it does not announce one.
pub fn sniff(contents: &str) -> Option<Format> {
    let start = contents.trim_start_matches(|c: char| c == '\u{feff}' || c.is_whitespace());
    match start.chars().next() {
        Some('[') | Some('{') => Some(Format::EnableBanking),
        _ => None,
    }
}

pub fn parse(contents: &str, format: Format) -> Result<Vec<Transaction>> {
    match format {
        Format::Seb => parse_seb(contents),
        Format::EnableBanking => parse_enable_banking(contents),
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
            reference: None,
        });
    }
    Ok(transactions)
}

/// The shape of one transaction in Enable Banking's JSON.
///
/// Only the fields this tool uses are named; the payload carries a dozen
/// more (`creditor`, `merchant_category_code`, `exchange_rate`), which
/// banks often leave null.
#[derive(Debug, Deserialize)]
struct EbTransaction {
    /// Absent on a pending transaction, which is why those are skipped.
    booking_date: Option<NaiveDate>,
    value_date: Option<NaiveDate>,
    /// The day a card was actually used, when the bank fills it in.
    transaction_date: Option<NaiveDate>,
    transaction_amount: EbAmount,
    /// `DBIT` for money out, `CRDT` for money in. The amount itself is
    /// unsigned, so this is the only thing that carries direction.
    credit_debit_indicator: Option<String>,
    /// `BOOK` once posted, `PDNG` while pending.
    status: Option<String>,
    entry_reference: Option<String>,
    #[serde(default)]
    remittance_information: Vec<String>,
    bank_transaction_code: Option<EbBankTransactionCode>,
    balance_after_transaction: Option<EbAmount>,
}

#[derive(Debug, Deserialize)]
struct EbAmount {
    amount: String,
}

#[derive(Debug, Deserialize)]
struct EbBankTransactionCode {
    /// "Card purchase", "Instant payment", "Mortgage" and so on. The CSV
    /// has no equivalent, so this is how a card purchase
    /// is recognised here — the CSV has to infer it from the descriptor's
    /// shape instead.
    description: Option<String>,
}

/// Either a bare array, or the `{"transactions": [...]}` the API returns.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EbPayload {
    Bare(Vec<EbTransaction>),
    Wrapped { transactions: Vec<EbTransaction> },
}

fn parse_enable_banking(contents: &str) -> Result<Vec<Transaction>> {
    let payload: EbPayload = serde_json::from_str(contents)
        .context("this does not look like an Enable Banking export")?;
    let rows = match payload {
        EbPayload::Bare(rows) => rows,
        EbPayload::Wrapped { transactions } => transactions,
    };

    let mut transactions = Vec::new();
    for (offset, row) in rows.into_iter().enumerate() {
        let number = offset + 1;

        // A pending transaction has no booking date and is replaced by a
        // booked one within a day or two, often with a different
        // descriptor. Counting both would double it.
        if row.status.as_deref() == Some("PDNG") {
            continue;
        }
        let Some(booked) = row.booking_date else {
            continue;
        };

        let magnitude = row
            .transaction_amount
            .amount
            .parse::<Amount>()
            .with_context(|| format!("transaction {number}: bad amount"))?;
        let amount = match row.credit_debit_indicator.as_deref() {
            Some("DBIT") => -magnitude,
            Some("CRDT") | None => magnitude,
            Some(other) => bail!("transaction {number}: unknown direction {other:?}"),
        };

        let text = row.remittance_information.join(" ").trim().to_string();
        let kind = row
            .bank_transaction_code
            .as_ref()
            .and_then(|code| code.description.as_deref());

        transactions.push(Transaction {
            booked,
            value_date: row.value_date.unwrap_or(booked),
            // The API has no posting batch. `entry_reference` identifies
            // the single transaction, which is not the same thing, so this
            // stays empty rather than holding something it does not mean.
            batch: String::new(),
            descriptor: enable_banking_descriptor(&text, kind, row.transaction_date),
            text,
            amount,
            // Present on every row, but with currency `XXX` — no currency
            // declared. The number itself matches the CSV's Saldo.
            balance: row
                .balance_after_transaction
                .and_then(|b| b.amount.parse::<Amount>().ok()),
            reference: row.entry_reference,
        });
    }
    Ok(transactions)
}

/// Classify a descriptor that has not been truncated.
///
/// The CSV has to guess from the shape of the text — twelve characters and
/// a slash means a card. Here the bank says so outright, which is both more
/// reliable and the only way to tell a card purchase from a bankgiro
/// payment once the `/YY-MM-DD` suffix is gone.
fn enable_banking_descriptor(
    text: &str,
    kind: Option<&str>,
    purchased: Option<NaiveDate>,
) -> Descriptor {
    let trimmed = text.trim();
    if kind == Some("Card purchase") {
        return Descriptor::Card {
            merchant: trimmed.to_string(),
            purchased,
        };
    }
    // A credit transfer carries a payment reference after whatever
    // identifies it — the counterparty's account, or the message typed
    // with the transfer:
    //
    //     12345678901 987654321012
    //     HYRA        123456789012
    //
    // The reference is unique per transaction, so keying on the whole
    // string would make every transfer its own merchant.
    let key = match kind {
        Some("Credit transfer") => strip_payment_reference(trimmed),
        _ => trimmed,
    };

    // A Swish payment arrives as the counterparty's number alone.
    if is_counterparty_number(key) {
        return Descriptor::Swish {
            number: key.to_string(),
        };
    }
    Descriptor::Plain {
        text: key.to_string(),
    }
}

/// Drop a trailing payment reference, leaving whatever came before it.
///
/// A reference is a long run of digits at the end, after whitespace. Nine
/// is the shortest seen; requiring that many keeps it from eating a house
/// number or a shop's branch number off the end of a real name.
fn strip_payment_reference(text: &str) -> &str {
    match text.rsplit_once(char::is_whitespace) {
        Some((head, reference))
            if reference.len() >= 9
                && reference.chars().all(|c| c.is_ascii_digit())
                && !head.trim().is_empty() =>
        {
            head.trim_end()
        }
        _ => text,
    }
}

/// Eight digits or more, and nothing else — a Swish number, or a bank
/// account in the form the counterparty field uses.
fn is_counterparty_number(text: &str) -> bool {
    text.len() >= 8 && text.chars().all(|c| c.is_ascii_digit())
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

    const EB_SAMPLE: &str = r#"[
      {"booking_date": "2026-09-11", "value_date": "2026-09-11", "transaction_date": null,
       "transaction_amount": {"currency": "SEK", "amount": "150.000"},
       "credit_debit_indicator": "DBIT", "status": "BOOK",
       "remittance_information": ["EXAMPLE CHARITY"],
       "bank_transaction_code": {"description": "Card purchase"},
       "balance_after_transaction": {"currency": "XXX", "amount": "1000.000"}},
      {"booking_date": "2026-09-10", "value_date": "2026-09-10", "transaction_date": null,
       "transaction_amount": {"currency": "SEK", "amount": "500.000"},
       "credit_debit_indicator": "CRDT", "status": "BOOK",
       "remittance_information": ["46700000001"],
       "bank_transaction_code": {"description": "Instant payment"},
       "balance_after_transaction": null},
      {"booking_date": "2026-09-09", "value_date": "2026-09-09", "transaction_date": null,
       "transaction_amount": {"currency": "SEK", "amount": "2000.000"},
       "credit_debit_indicator": "DBIT", "status": "BOOK",
       "remittance_information": ["12345678901 987654321012"],
       "bank_transaction_code": {"description": "Credit transfer"},
       "balance_after_transaction": null},
      {"booking_date": "2026-09-08", "value_date": "2026-09-08", "transaction_date": null,
       "transaction_amount": {"currency": "SEK", "amount": "700.000"},
       "credit_debit_indicator": "DBIT", "status": "BOOK",
       "remittance_information": ["HYRA        123456789012"],
       "bank_transaction_code": {"description": "Credit transfer"},
       "balance_after_transaction": null},
      {"booking_date": null, "value_date": null, "transaction_date": null,
       "transaction_amount": {"currency": "SEK", "amount": "50.000"},
       "credit_debit_indicator": "DBIT", "status": "PDNG",
       "remittance_information": ["123456789", "KORTBOLAGET AB"],
       "bank_transaction_code": null, "balance_after_transaction": null}
    ]"#;

    #[test]
    fn reads_the_enable_banking_export() {
        let transactions = parse(EB_SAMPLE, Format::EnableBanking).unwrap();
        // The pending row is not one of them.
        assert_eq!(transactions.len(), 4);

        let first = &transactions[0];
        assert_eq!(first.booked, NaiveDate::from_ymd_opt(2026, 9, 11).unwrap());
        assert_eq!(first.amount, "-150.00".parse().unwrap());
        assert_eq!(first.balance, Some("1000.000".parse().unwrap()));
        assert_eq!(
            first.descriptor,
            Descriptor::Card {
                merchant: "EXAMPLE CHARITY".into(),
                purchased: None,
            }
        );
    }

    /// The amount is unsigned; only `credit_debit_indicator` says which way
    /// the money went.
    #[test]
    fn takes_the_direction_from_the_indicator() {
        let transactions = parse(EB_SAMPLE, Format::EnableBanking).unwrap();
        assert!(transactions[0].amount.is_negative());
        assert_eq!(transactions[1].amount, "500".parse().unwrap());
    }

    /// The bank says outright that a row is a card purchase, so an
    /// untruncated merchant name does not have to be guessed at.
    #[test]
    fn classifies_by_the_bank_transaction_code() {
        let transactions = parse(EB_SAMPLE, Format::EnableBanking).unwrap();
        assert_eq!(transactions[0].descriptor.kind(), "card");
        assert_eq!(
            transactions[1].descriptor,
            Descriptor::Swish {
                number: "46700000001".into()
            }
        );
    }

    /// A credit transfer carries a payment reference after the account
    /// number, unique to the transaction. Keying on it would make every
    /// transfer its own merchant.
    #[test]
    fn drops_the_payment_reference_from_a_transfer() {
        let transactions = parse(EB_SAMPLE, Format::EnableBanking).unwrap();
        assert_eq!(
            transactions[2].descriptor,
            Descriptor::Swish {
                number: "12345678901".into()
            }
        );
    }

    /// The reference follows a typed message just as it follows an
    /// account number, and the message is the part worth keying on.
    #[test]
    fn drops_the_reference_after_a_typed_message_too() {
        let transactions = parse(EB_SAMPLE, Format::EnableBanking).unwrap();
        assert_eq!(
            transactions[3].descriptor,
            Descriptor::Plain {
                text: "HYRA".into()
            }
        );
    }

    /// Only a credit transfer carries one, and only a long run of digits
    /// is one — a shop with a number in its name keeps it.
    #[test]
    fn leaves_a_number_that_is_part_of_the_name() {
        assert_eq!(
            enable_banking_descriptor("KIOSKEN 1234567", Some("Card purchase"), None),
            Descriptor::Card {
                merchant: "KIOSKEN 1234567".into(),
                purchased: None,
            }
        );
        assert_eq!(strip_payment_reference("BUTIKEN 4242"), "BUTIKEN 4242");
        assert_eq!(strip_payment_reference("HYRA 123456789012"), "HYRA");
        assert_eq!(strip_payment_reference("123456789012"), "123456789012");
    }

    #[test]
    fn reads_the_wrapped_shape_the_api_returns() {
        let wrapped = format!(r#"{{"transactions": {EB_SAMPLE}}}"#);
        let transactions = parse(&wrapped, Format::EnableBanking).unwrap();
        assert_eq!(transactions.len(), 4);
    }

    #[test]
    fn sniffs_json_but_leaves_everything_else_alone() {
        assert_eq!(sniff(EB_SAMPLE), Some(Format::EnableBanking));
        assert_eq!(sniff("  \n [ ]"), Some(Format::EnableBanking));
        assert_eq!(
            sniff(r#"{"transactions": []}"#),
            Some(Format::EnableBanking)
        );
        assert_eq!(sniff(SAMPLE), None);
        assert_eq!(sniff(""), None);
    }

    #[test]
    fn rejects_json_that_is_not_a_statement() {
        let error = parse(r#"{"hello": "world"}"#, Format::EnableBanking).unwrap_err();
        assert!(error.to_string().contains("does not look like"));
    }

    #[test]
    fn rejects_a_file_that_is_not_a_statement() {
        let error = parse("name,value\nfoo,1\n", Format::Seb).unwrap_err();
        assert!(error.to_string().contains("does not look like"));
    }
}
