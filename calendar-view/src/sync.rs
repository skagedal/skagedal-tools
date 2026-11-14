//! Runs the configured sync command (`neverest sync`, typically) at start,
//! then every so often while the view is open, and when the page asks.

use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub configured: bool,
    pub running: bool,
    pub last_ok: Option<DateTime<Local>>,
    pub last_error: Option<String>,
}

pub struct Syncer {
    status: Mutex<Status>,
    wake: Condvar,
    asked: Mutex<bool>,
}

impl Syncer {
    pub fn start(command: Vec<String>, every: Duration) -> Arc<Syncer> {
        let syncer = Arc::new(Syncer {
            status: Mutex::new(Status {
                configured: !command.is_empty(),
                ..Status::default()
            }),
            wake: Condvar::new(),
            asked: Mutex::new(false),
        });
        if command.is_empty() {
            return syncer;
        }
        let worker = syncer.clone();
        std::thread::spawn(move || {
            loop {
                worker.run(&command);
                let asked = worker.asked.lock().unwrap();
                let (mut asked, _) = worker
                    .wake
                    .wait_timeout_while(asked, every, |asked| !*asked)
                    .unwrap();
                *asked = false;
            }
        });
        syncer
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    /// Sync now, or as soon as the one running ends.
    pub fn ask(&self) {
        *self.asked.lock().unwrap() = true;
        self.wake.notify_one();
    }

    fn run(&self, command: &[String]) {
        self.status.lock().unwrap().running = true;
        let result = Command::new(&command[0]).args(&command[1..]).output();
        let mut status = self.status.lock().unwrap();
        status.running = false;
        match result {
            Ok(out) if out.status.success() => {
                status.last_ok = Some(Local::now());
                status.last_error = None;
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let last = stderr.lines().rev().find(|l| !l.trim().is_empty());
                status.last_error = Some(format!(
                    "{} exited with {}: {}",
                    command.join(" "),
                    out.status,
                    last.unwrap_or("no output")
                ));
            }
            Err(e) => status.last_error = Some(format!("running {}: {e}", command[0])),
        }
    }
}
