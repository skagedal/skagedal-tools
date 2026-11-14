//! The occurrences of a set of calendars over a range of days, read in
//! parallel and kept until the store they come from changes.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;

use crate::calendula::{self, Calendar, Calendula, Failure};
use crate::settings::{self, Settings};

/// How long events are kept when no `watch` file says when they changed.
const UNWATCHED_TTL: Duration = Duration::from_secs(300);

pub struct Agenda {
    calendula: Calendula,
    settings: Settings,
    pub calendars: Vec<Calendar>,
    watch: Vec<PathBuf>,
    cache: Mutex<Cache>,
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Each with `calendar`, the key of the calendar it is from.
    pub events: Vec<Value>,
    pub failures: Vec<Failure>,
}

type Read = std::result::Result<Vec<Value>, String>;

struct Cache {
    fingerprint: String,
    since: Instant,
    entries: HashMap<(usize, NaiveDate, NaiveDate), Read>,
}

impl Agenda {
    pub fn new(settings: Settings) -> Result<Self> {
        let calendula = Calendula::new(&settings);
        let calendars = calendula::catalog(&calendula, &settings)?;
        let watch: Vec<PathBuf> = settings.watch.iter().map(|w| settings::expand(w)).collect();
        Ok(Agenda {
            cache: Mutex::new(Cache {
                fingerprint: fingerprint(&watch),
                since: Instant::now(),
                entries: HashMap::new(),
            }),
            calendula,
            settings,
            calendars,
            watch,
        })
    }

    /// Changes whenever a watched file does, or every few minutes with none.
    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    pub fn version(&self) -> String {
        let mut cache = self.cache.lock().unwrap();
        self.expire(&mut cache);
        cache.fingerprint.clone()
    }

    fn expire(&self, cache: &mut Cache) {
        let now = fingerprint(&self.watch);
        let stale = if self.watch.is_empty() {
            cache.since.elapsed() > UNWATCHED_TTL
        } else {
            now != cache.fingerprint
        };
        if stale {
            cache.entries.clear();
            cache.since = Instant::now();
            cache.fingerprint = if self.watch.is_empty() {
                format!("{:?}", SystemTime::now())
            } else {
                now
            };
        }
    }

    /// The calendars at `which` (indices into [`Agenda::calendars`]), from
    /// `from` to `to`, both days included.
    pub fn overview(&self, which: &[usize], from: NaiveDate, to: NaiveDate) -> Overview {
        let which: Vec<usize> = which
            .iter()
            .copied()
            .filter(|&i| i < self.calendars.len())
            .collect();
        let missing: Vec<usize> = {
            let mut cache = self.cache.lock().unwrap();
            self.expire(&mut cache);
            which
                .iter()
                .copied()
                .filter(|&i| !cache.entries.contains_key(&(i, from, to)))
                .collect()
        };
        let read: Vec<(usize, Read)> = std::thread::scope(|s| {
            let handles: Vec<_> = missing
                .iter()
                .map(|&i| s.spawn(move || (i, self.read(i, from, to))))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let mut cache = self.cache.lock().unwrap();
        for (i, result) in read {
            cache.entries.insert((i, from, to), result);
        }
        let mut events = Vec::new();
        let mut failures = Vec::new();
        for &i in &which {
            let calendar = &self.calendars[i];
            match cache.entries.get(&(i, from, to)) {
                Some(Ok(found)) => events.extend(found.iter().cloned()),
                Some(Err(error)) => failures.push(Failure {
                    calendar: calendar.key.clone(),
                    error: error.clone(),
                }),
                None => {}
            }
        }
        events.sort_by_key(calendula::sort_key);
        Overview {
            from,
            to,
            events,
            failures,
        }
    }

    fn read(&self, i: usize, from: NaiveDate, to: NaiveDate) -> Read {
        let calendar = &self.calendars[i];
        let me: &[String] = self
            .settings
            .accounts
            .get(&calendar.account)
            .map_or(&[], |a| &a.me);
        let events = self
            .calendula
            .events(&calendar.account, &calendar.id, from, to)
            .map_err(|e| format!("{e:#}"))?;
        Ok(events
            .into_iter()
            .filter(|e| calendula::keep(e, me))
            .map(|mut e| {
                e["calendar"] = Value::String(calendar.key.clone());
                e
            })
            .collect())
    }

    pub fn shown(&self) -> Vec<usize> {
        (0..self.calendars.len())
            .filter(|&i| self.calendars[i].shown)
            .collect()
    }
}

fn fingerprint(files: &[PathBuf]) -> String {
    let mut hasher = DefaultHasher::new();
    for file in files {
        let stamp = std::fs::metadata(file)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        stamp.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}
