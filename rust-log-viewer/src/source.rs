use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

use anyhow::Result;

use crate::entry::Entry;

#[derive(Debug, Clone)]
pub enum SourceSpec {
    File(PathBuf),
    Stdin,
}

impl SourceSpec {
    pub fn label(&self) -> &str {
        match self {
            Self::File(p) => p.to_str().unwrap_or("<file>"),
            Self::Stdin => "stdin",
        }
    }
}

pub fn load_entries(spec: &SourceSpec) -> Result<Vec<Entry>> {
    match spec {
        SourceSpec::File(path) => read_lines(BufReader::new(File::open(path)?)),
        SourceSpec::Stdin => read_lines(io::stdin().lock()),
    }
}

fn read_lines<R: BufRead>(reader: R) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        entries.push(Entry::parse(&line));
    }
    Ok(entries)
}

#[cfg(test)]
pub fn entries_from_str(input: &str) -> Result<Vec<Entry>> {
    read_lines(io::Cursor::new(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_blank_lines() {
        let entries = entries_from_str("\n{\"a\":1}\n\n{\"a\":2}\n").unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn label_for_stdin() {
        assert_eq!(SourceSpec::Stdin.label(), "stdin");
    }

    #[test]
    fn label_for_file() {
        let spec = SourceSpec::File(PathBuf::from("/tmp/x.jsonl"));
        assert_eq!(spec.label(), "/tmp/x.jsonl");
    }
}
