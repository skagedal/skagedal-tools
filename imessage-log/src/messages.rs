//! Reading `~/Library/Messages/chat.db`, the Messages app's own database.
//!
//! The database is opened read-only and never written. Its schema is
//! Apple's and undocumented; the parts used here — `message`, `handle`,
//! `chat` and the two join tables — have been stable for years.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};

/// Seconds from the Unix epoch to 2001-01-01, where Apple's dates start.
const APPLE_EPOCH: i64 = 978_307_200;

/// One message, as much of it as a log needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub sent: DateTime<Local>,
    pub from_me: bool,
    /// The other party's handle — a phone number or an email address —
    /// for a message someone else sent. Empty for my own.
    pub sender: String,
    /// Everyone else in the conversation, by handle.
    pub participants: Vec<String>,
    /// A group chat's name, when it has one.
    pub chat_name: Option<String>,
    pub group: bool,
    pub text: String,
    pub attachments: bool,
}

pub fn open(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("could not open {}", path.display()))
        .with_context(
            || "the Messages database needs Full Disk Access; run this from a terminal that has it",
        )
}

/// Every message sent between `from` and `to`, oldest first. Tapbacks and
/// other reactions are left out: they are not messages anyone wrote.
pub fn read(db: &Connection, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<Message>> {
    let participants = participants(db)?;
    let mut stmt = db.prepare(
        "SELECT m.date, m.is_from_me, m.text, m.attributedBody, m.cache_has_attachments,
                h.id, c.ROWID, c.display_name, c.style
         FROM message m
         LEFT JOIN handle h ON h.ROWID = m.handle_id
         LEFT JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
         LEFT JOIN chat c ON c.ROWID = cmj.chat_id
         WHERE m.date >= ?1 AND m.date < ?2
           AND COALESCE(m.associated_message_type, 0) = 0
         ORDER BY m.date",
    )?;
    let rows = stmt.query_map([to_apple(from), to_apple(to)], |row| {
        let date: i64 = row.get(0)?;
        let from_me: bool = row.get::<_, Option<i64>>(1)?.unwrap_or(0) != 0;
        let text: Option<String> = row.get(2)?;
        let body: Option<Vec<u8>> = row.get(3)?;
        let attachments = row.get::<_, Option<i64>>(4)?.unwrap_or(0) != 0;
        let handle: Option<String> = row.get(5)?;
        let chat: Option<i64> = row.get(6)?;
        let chat_name: Option<String> = row.get(7)?;
        let style: Option<i64> = row.get(8)?;
        Ok((
            date,
            from_me,
            text,
            body,
            attachments,
            handle,
            chat,
            chat_name,
            style,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (date, from_me, text, body, attachments, handle, chat, chat_name, style) = row?;
        let text = text
            .filter(|t| !t.trim().is_empty())
            .or_else(|| body.as_deref().and_then(decode_attributed_body))
            .unwrap_or_default()
            // U+FFFC stands in for an attachment inside the text.
            .replace('\u{fffc}', "")
            .trim()
            .to_string();
        if text.is_empty() && !attachments {
            continue;
        }
        out.push(Message {
            sent: from_apple(date),
            from_me,
            sender: if from_me {
                String::new()
            } else {
                handle.unwrap_or_default()
            },
            participants: chat
                .and_then(|c| participants.get(&c).cloned())
                .unwrap_or_default(),
            chat_name: chat_name.filter(|n| !n.trim().is_empty()),
            // 43 is a group chat, 45 a conversation with one person.
            group: style == Some(43),
            text,
            attachments,
        });
    }
    Ok(out)
}

fn participants(db: &Connection) -> Result<HashMap<i64, Vec<String>>> {
    let mut stmt = db.prepare(
        "SELECT chj.chat_id, h.id FROM chat_handle_join chj JOIN handle h ON h.ROWID = chj.handle_id",
    )?;
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (chat, handle) = row?;
        let list = map.entry(chat).or_default();
        if !list.contains(&handle) {
            list.push(handle);
        }
    }
    Ok(map)
}

/// Apple stores nanoseconds since 2001 in current databases and seconds in
/// old ones; a value too large to be seconds is nanoseconds.
fn from_apple(value: i64) -> DateTime<Local> {
    let (secs, nanos) = if value.abs() > 10_000_000_000 {
        (value / 1_000_000_000, (value % 1_000_000_000) as u32)
    } else {
        (value, 0)
    };
    Utc.timestamp_opt(secs + APPLE_EPOCH, nanos)
        .single()
        .unwrap_or_default()
        .with_timezone(&Local)
}

fn to_apple(t: DateTime<Utc>) -> i64 {
    (t.timestamp() - APPLE_EPOCH) * 1_000_000_000 + i64::from(t.timestamp_subsec_nanos())
}

/// Since macOS Ventura the text of many messages is only in
/// `attributedBody`: an NSAttributedString in Apple's old typedstream
/// format. The string is the first `NSString` in it, after a five-byte
/// preamble, prefixed with its length — one byte, or 0x81 and two bytes
/// little-endian, or 0x82 and four.
pub fn decode_attributed_body(body: &[u8]) -> Option<String> {
    let marker = b"NSString";
    let at = body.windows(marker.len()).position(|w| w == marker)?;
    let rest = body.get(at + marker.len() + 5..)?;
    let (len, start) = match *rest.first()? {
        0x81 => (
            u16::from_le_bytes([*rest.get(1)?, *rest.get(2)?]) as usize,
            3,
        ),
        0x82 => (
            u32::from_le_bytes([*rest.get(1)?, *rest.get(2)?, *rest.get(3)?, *rest.get(4)?])
                as usize,
            5,
        ),
        n => (n as usize, 1),
    };
    let bytes = rest.get(start..start + len)?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn body(text: &str) -> Vec<u8> {
        let mut b = b"\x04\x0bstreamtyped\x81\xe8\x03\x84\x01@\x84\x84\x84\x12NSAttributedString\x00\x84\x84\x08NSObject\x00\x85\x92\x84\x84\x84\x08NSString\x01\x94\x84\x01+".to_vec();
        let bytes = text.as_bytes();
        if bytes.len() < 0x80 {
            b.push(bytes.len() as u8);
        } else {
            b.push(0x81);
            b.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        }
        b.extend_from_slice(bytes);
        b.extend_from_slice(b"\x86\x84\x02iI\x01");
        b
    }

    #[test]
    fn decodes_a_short_attributed_body() {
        assert_eq!(
            decode_attributed_body(&body("hej!")).as_deref(),
            Some("hej!")
        );
    }

    #[test]
    fn decodes_a_long_one_with_a_two_byte_length() {
        let long = "å".repeat(200);
        assert_eq!(decode_attributed_body(&body(&long)), Some(long));
    }

    #[test]
    fn gives_up_on_something_else() {
        assert_eq!(decode_attributed_body(b"not a typedstream"), None);
    }

    #[test]
    fn apple_dates_round_trip_and_old_seconds_are_understood() {
        let t = Utc.with_ymd_and_hms(2026, 4, 10, 17, 32, 0).unwrap();
        assert_eq!(from_apple(to_apple(t)).with_timezone(&Utc), t);
        let seconds = t.timestamp() - APPLE_EPOCH;
        assert_eq!(from_apple(seconds).with_timezone(&Utc), t);
    }

    /// The parts of Apple's schema this reads, with a made-up conversation.
    pub(crate) fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
             CREATE TABLE chat (ROWID INTEGER PRIMARY KEY, display_name TEXT, style INTEGER);
             CREATE TABLE message (ROWID INTEGER PRIMARY KEY, date INTEGER, is_from_me INTEGER,
               text TEXT, attributedBody BLOB, cache_has_attachments INTEGER,
               handle_id INTEGER, associated_message_type INTEGER);
             CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
             CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
             INSERT INTO handle VALUES (1, '+46700000001'), (2, 'someone@example.com');
             INSERT INTO chat VALUES (1, '', 45), (2, 'Book club', 43);
             INSERT INTO chat_handle_join VALUES (1, 1), (2, 1), (2, 2);",
        )
        .unwrap();
        let at = |h: u32, m: u32| {
            to_apple(
                Local
                    .with_ymd_and_hms(2026, 4, 10, h, m, 0)
                    .unwrap()
                    .with_timezone(&Utc),
            )
        };
        let insert = |id: i64,
                      date: i64,
                      me: i64,
                      text: Option<&str>,
                      body: Option<Vec<u8>>,
                      att: i64,
                      handle: i64,
                      reaction: i64,
                      chat: i64| {
            db.execute(
                "INSERT INTO message VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![id, date, me, text, body, att, handle, reaction],
            )
            .unwrap();
            db.execute("INSERT INTO chat_message_join VALUES (?1, ?2)", [chat, id])
                .unwrap();
        };
        insert(
            1,
            at(19, 0),
            0,
            Some("Where shall we eat?"),
            None,
            0,
            1,
            0,
            1,
        );
        insert(
            2,
            at(19, 5),
            1,
            None,
            Some(body("The usual place")),
            0,
            0,
            0,
            1,
        );
        insert(
            3,
            at(19, 6),
            0,
            Some("Loved “The usual place”"),
            None,
            0,
            1,
            2000,
            1,
        );
        insert(4, at(20, 0), 0, Some("\u{fffc}"), None, 1, 2, 0, 2);
        insert(5, at(23, 0), 0, Some("Next day almost"), None, 0, 2, 0, 2);
        (dir, path)
    }

    #[test]
    fn reads_a_window_leaving_out_reactions() {
        let (_dir, path) = fixture();
        let db = open(&path).unwrap();
        let from = Local
            .with_ymd_and_hms(2026, 4, 10, 0, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        let to = Local
            .with_ymd_and_hms(2026, 4, 10, 22, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        let all = read(&db, from, to).unwrap();
        let texts: Vec<_> = all.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["Where shall we eat?", "The usual place", ""]);
        assert!(all[1].from_me);
        assert_eq!(all[0].sender, "+46700000001");
        assert!(all[2].attachments && all[2].group);
        assert_eq!(all[2].chat_name.as_deref(), Some("Book club"));
        assert_eq!(all[2].participants, ["+46700000001", "someone@example.com"]);
    }
}
