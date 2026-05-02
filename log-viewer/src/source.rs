use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
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
    /// The executed subprocess for `SourceSpec::Command`, so we can kill it
    /// when log-viewer exits — otherwise the child (e.g. stern) keeps
    /// streaming after the TUI is gone.
    child: Option<Child>,
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

impl Drop for EntryStream {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn start(spec: &SourceSpec, default_field: &str) -> Result<EntryStream> {
    let (tx, rx) = channel();
    let default_field = default_field.to_string();
    let mut child_handle: Option<Child> = None;
    match spec {
        SourceSpec::File(path) => {
            let reader = BufReader::new(
                File::open(path).with_context(|| format!("opening {}", path.display()))?,
            );
            spawn_reader(reader, tx, default_field, false);
        }
        SourceSpec::Stdin => {
            spawn_reader(BufReader::new(io::stdin()), tx, default_field, false);
        }
        SourceSpec::Command(argv) => {
            let mut cmd_iter = argv.iter();
            let program = cmd_iter
                .next()
                .ok_or_else(|| anyhow::anyhow!("--exec requires a command"))?;
            let mut cmd = Command::new(program);
            cmd.args(cmd_iter)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null());
            let mut child = cmd
                .spawn()
                .with_context(|| format!("launching {}", program.to_string_lossy()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("child stdout missing"))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("child stderr missing"))?;
            spawn_reader(BufReader::new(stdout), tx.clone(), default_field.clone(), false);
            spawn_reader(BufReader::new(stderr), tx, default_field, true);
            child_handle = Some(child);
        }
    }
    Ok(EntryStream {
        rx,
        child: child_handle,
    })
}

fn spawn_reader<R: BufRead + Send + 'static>(
    reader: R,
    tx: Sender<Entry>,
    default_field: String,
    is_stderr: bool,
) {
    thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else { return };
            if line.is_empty() {
                continue;
            }
            let entry = if is_stderr {
                Entry::parse_stderr(&line, &default_field)
            } else {
                Entry::parse(&line, &default_field)
            };
            if tx.send(entry).is_err() {
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

    #[test]
    fn command_source_captures_stderr_as_is_stderr_entries() {
        let spec = SourceSpec::Command(
            [
                "sh",
                "-c",
                "printf '{\"k\":\"out\"}\\n' >&1; printf 'oops on stderr\\n' >&2",
            ]
            .iter()
            .map(|s| OsString::from(*s))
            .collect(),
        );
        let stream = start(&spec, "message").expect("start");
        // Both reader threads need time to push their lines.
        let mut entries = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while entries.len() < 2 && std::time::Instant::now() < deadline {
            entries.extend(stream.drain());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(entries.len(), 2, "expected stdout + stderr entries");
        let stderr = entries.iter().find(|e| e.is_stderr).expect("stderr entry");
        assert_eq!(stderr.raw, "oops on stderr");
        let stdout = entries.iter().find(|e| !e.is_stderr).expect("stdout entry");
        assert_eq!(stdout.get_str("k").unwrap(), "out");
    }

    #[test]
    fn command_source_kills_long_running_child_on_drop() {
        // A sleeper that prints its own pid and stays alive. Dropping the
        // EntryStream should kill it; we verify by checking the pid is gone.
        let spec = SourceSpec::Command(
            ["sh", "-c", "echo $$; sleep 30"]
                .iter()
                .map(|s| OsString::from(*s))
                .collect(),
        );
        let stream = start(&spec, "message").expect("start");
        // Wait for the pid line.
        let mut pid: Option<i32> = None;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while pid.is_none() && std::time::Instant::now() < deadline {
            for e in stream.drain() {
                if !e.is_stderr {
                    pid = e.raw.trim().parse().ok();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let pid = pid.expect("child should print its pid");
        // Sanity: process exists.
        assert!(
            unsafe { libc_kill(pid, 0) } == 0,
            "child {pid} should be alive before drop"
        );
        drop(stream);
        // Give the OS a moment to reap.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if unsafe { libc_kill(pid, 0) } != 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("child {pid} still alive after EntryStream drop");
    }

    // Tiny libc::kill wrapper so the test doesn't need a libc dependency.
    unsafe extern "C" {
        #[link_name = "kill"]
        fn libc_kill(pid: i32, sig: i32) -> i32;
    }
}
