use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use anyhow::{Context, Result};

use crate::entry::Entry;

#[derive(Debug, Clone)]
pub enum SourceSpec {
    File(PathBuf),
    Stdin,
    Command(Vec<OsString>),
}

impl SourceSpec {
    pub fn label(&self) -> String {
        match self {
            Self::File(p) => p.to_string_lossy().into_owned(),
            Self::Stdin => "stdin".into(),
            Self::Command(argv) => format_command(argv),
        }
    }
}

fn format_command(argv: &[OsString]) -> String {
    let mut out = String::from("$ ");
    for (i, a) in argv.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let s = a.to_string_lossy();
        if s.contains(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '\\' | '$')) {
            out.push('"');
            out.push_str(&s.replace('"', "\\\""));
            out.push('"');
        } else {
            out.push_str(&s);
        }
    }
    out
}

/// A live source of entries. Producer threads push parsed entries into the
/// channel; the consumer drains it on each tick.
pub struct EntryStream {
    rx: Receiver<Entry>,
}

impl EntryStream {
    /// Drain all currently-available entries (non-blocking).
    pub fn drain(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            out.push(e);
        }
        out
    }
}

pub fn start(spec: &SourceSpec, default_field: &str) -> Result<EntryStream> {
    let (tx, rx) = channel();
    let default_field = default_field.to_string();
    match spec {
        SourceSpec::File(path) => {
            let reader = BufReader::new(
                File::open(path).with_context(|| format!("opening {}", path.display()))?,
            );
            spawn_reader(reader, tx, default_field);
        }
        SourceSpec::Stdin => {
            spawn_reader(BufReader::new(io::stdin()), tx, default_field);
        }
        SourceSpec::Command(argv) => {
            let mut cmd_iter = argv.iter();
            let program = cmd_iter
                .next()
                .ok_or_else(|| anyhow::anyhow!("--exec requires a command"))?;
            let mut cmd = Command::new(program);
            cmd.args(cmd_iter)
                .stdout(Stdio::piped())
                .stdin(Stdio::null());
            let mut child = cmd
                .spawn()
                .with_context(|| format!("launching {}", program.to_string_lossy()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("child stdout missing"))?;
            spawn_reader(BufReader::new(stdout), tx, default_field);
            // Reap the child in the background so it doesn't become a zombie.
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
    Ok(EntryStream { rx })
}

fn spawn_reader<R: BufRead + Send + 'static>(
    reader: R,
    tx: Sender<Entry>,
    default_field: String,
) {
    thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else { return };
            if line.is_empty() {
                continue;
            }
            if tx.send(Entry::parse(&line, &default_field)).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_for_stdin() {
        assert_eq!(SourceSpec::Stdin.label(), "stdin");
    }

    #[test]
    fn label_for_file() {
        let spec = SourceSpec::File(PathBuf::from("/tmp/x.jsonl"));
        assert_eq!(spec.label(), "/tmp/x.jsonl");
    }

    #[test]
    fn label_for_command_quotes_args() {
        let spec = SourceSpec::Command(
            ["kubectl", "logs", "-f", "my pod"]
                .iter()
                .map(|s| OsString::from(*s))
                .collect(),
        );
        assert_eq!(spec.label(), "$ kubectl logs -f \"my pod\"");
    }
}
