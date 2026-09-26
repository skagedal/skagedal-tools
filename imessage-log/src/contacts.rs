//! Names for the phone numbers and email addresses in the Messages
//! database, from the Contacts app's own databases.
//!
//! Contacts keeps one SQLite file per account under
//! `~/Library/Application Support/AddressBook/`: one at the top, and one in
//! each directory under `Sources/`. All of them are read, read-only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{Connection, OpenFlags};

const FILE: &str = "AddressBook-v22.abcddb";

/// Handle to name. Empty when Contacts could not be read, in which case
/// every handle shows as itself.
#[derive(Debug, Default)]
pub struct Contacts {
    names: HashMap<String, String>,
}

impl Contacts {
    pub fn load(address_book: &Path) -> Result<Contacts> {
        let mut contacts = Contacts::default();
        for file in databases(address_book) {
            contacts.read(&file)?;
        }
        Ok(contacts)
    }

    fn read(&mut self, file: &Path) -> Result<()> {
        let db = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let names = record_names(&db)?;
        for (sql, is_phone) in [
            ("SELECT ZOWNER, ZFULLNUMBER FROM ZABCDPHONENUMBER", true),
            ("SELECT ZOWNER, ZADDRESS FROM ZABCDEMAILADDRESS", false),
        ] {
            let mut stmt = db.prepare(sql)?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<String>>(1)?))
            })?;
            for row in rows {
                let (Some(owner), Some(value)) = row? else {
                    continue;
                };
                if let Some(name) = names.get(&owner) {
                    let key = if is_phone {
                        normalize(&value)
                    } else {
                        value.trim().to_lowercase()
                    };
                    self.names.entry(key).or_insert_with(|| name.clone());
                }
            }
        }
        Ok(())
    }

    /// The name for a handle, or the handle itself.
    pub fn name<'a>(&'a self, handle: &'a str) -> &'a str {
        self.names
            .get(&normalize(handle))
            .map(String::as_str)
            .unwrap_or(handle)
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

fn record_names(db: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = db
        .prepare("SELECT Z_PK, ZFIRSTNAME, ZLASTNAME, ZORGANIZATION, ZNICKNAME FROM ZABCDRECORD")?;
    let mut names = HashMap::new();
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    for row in rows {
        let (pk, first, last, org, nick) = row?;
        let person = [first, last]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let name = Some(person)
            .filter(|s| !s.is_empty())
            .or(org)
            .or(nick)
            .filter(|s| !s.trim().is_empty());
        if let Some(name) = name {
            names.insert(pk, name);
        }
    }
    Ok(names)
}

fn databases(address_book: &Path) -> Vec<PathBuf> {
    let mut files = vec![address_book.join(FILE)];
    if let Ok(entries) = std::fs::read_dir(address_book.join("Sources")) {
        files.extend(entries.flatten().map(|e| e.path().join(FILE)));
    }
    files.into_iter().filter(|f| f.exists()).collect()
}

/// One spelling for a phone number, whichever way it was written: digits
/// only, with the country code. A number without one is taken as Swedish,
/// since that is how people write them here.
pub fn normalize(handle: &str) -> String {
    let handle = handle.trim();
    if handle.contains('@') {
        return handle.to_lowercase();
    }
    let digits: String = handle.chars().filter(char::is_ascii_digit).collect();
    if handle.starts_with('+') {
        digits
    } else if let Some(rest) = digits.strip_prefix("00") {
        rest.to_string()
    } else if let Some(rest) = digits.strip_prefix('0') {
        format!("46{rest}")
    } else {
        digits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_number_however_it_is_written() {
        for written in [
            "+46 70 000 00 01",
            "070-000 00 01",
            "0046700000001",
            "+46700000001",
        ] {
            assert_eq!(normalize(written), "46700000001", "{written}");
        }
        assert_eq!(normalize(" Someone@Example.com"), "someone@example.com");
    }

    #[test]
    fn reads_names_from_every_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Sources").join("ABC");
        std::fs::create_dir_all(&source).unwrap();
        let db = Connection::open(source.join(FILE)).unwrap();
        db.execute_batch(
            "CREATE TABLE ZABCDRECORD (Z_PK INTEGER PRIMARY KEY, ZFIRSTNAME TEXT, ZLASTNAME TEXT,
               ZORGANIZATION TEXT, ZNICKNAME TEXT);
             CREATE TABLE ZABCDPHONENUMBER (ZOWNER INTEGER, ZFULLNUMBER TEXT);
             CREATE TABLE ZABCDEMAILADDRESS (ZOWNER INTEGER, ZADDRESS TEXT);
             INSERT INTO ZABCDRECORD VALUES (1, 'Ada', 'Lovelace', NULL, NULL),
                                            (2, NULL, NULL, 'The Bakery', NULL);
             INSERT INTO ZABCDPHONENUMBER VALUES (1, '070-000 00 01');
             INSERT INTO ZABCDEMAILADDRESS VALUES (2, 'Orders@Example.com');",
        )
        .unwrap();
        let contacts = Contacts::load(dir.path()).unwrap();
        assert_eq!(contacts.name("+46700000001"), "Ada Lovelace");
        assert_eq!(contacts.name("orders@example.com"), "The Bakery");
        assert_eq!(contacts.name("+46799999999"), "+46799999999");
    }
}
