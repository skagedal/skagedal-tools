//! Everything read comes from `calendula … --json`, so the view shows what
//! the CLI shows: the same occurrences, expanded by the same code.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::Settings;

pub struct Calendula {
    bin: String,
    config: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Listed {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// A calendar as the page sees it.
#[derive(Debug, Clone, Serialize)]
pub struct Calendar {
    pub key: String,
    pub account: String,
    pub id: String,
    pub name: String,
    pub label: String,
    pub color: String,
    pub shown: bool,
}

/// One calendar that could not be read, so a gap is never taken for a quiet day.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub calendar: String,
    pub error: String,
}

/// Assigned in order to calendars with no colour of their own.
const PALETTE: &[&str] = &[
    "#2a78d6", "#d0663b", "#0c9c6b", "#9b59d0", "#c9a400", "#d03b8a", "#3bb5d0", "#7a7a72",
];

impl Calendula {
    pub fn new(settings: &Settings) -> Self {
        Calendula {
            bin: settings
                .calendula
                .clone()
                .unwrap_or_else(|| "calendula".into()),
            config: settings
                .config
                .iter()
                .map(|c| crate::settings::expand(c).to_string_lossy().into_owned())
                .collect(),
        }
    }

    fn run(&self, args: &[&str]) -> Result<Value> {
        let mut command = Command::new(&self.bin);
        for config in &self.config {
            command.args(["-c", config]);
        }
        let out = command
            .args(args)
            .arg("--json")
            .output()
            .with_context(|| format!("running {}", self.bin))?;
        let parsed: Option<Value> = serde_json::from_slice(&out.stdout).ok();
        if !out.status.success() {
            let message = parsed
                .as_ref()
                .and_then(|v| v.get("error"))
                .and_then(|e| {
                    e.as_str()
                        .map(str::to_owned)
                        .or_else(|| Some(e.to_string()))
                })
                .unwrap_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string());
            let first = message.lines().next().unwrap_or("failed");
            bail!("calendula {}: {first}", args.join(" "));
        }
        parsed.ok_or_else(|| anyhow!("calendula {}: output is not JSON", args.join(" ")))
    }

    pub fn accounts(&self) -> Result<Vec<String>> {
        let v = self.run(&["account", "list"])?;
        Ok(v["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["name"].as_str().map(str::to_owned))
            .collect())
    }

    pub fn calendars(&self, account: &str) -> Result<Vec<Listed>> {
        let v = self.run(&["-a", account, "calendar", "list"])?;
        serde_json::from_value(v["calendars"].clone()).context("reading the calendar list")
    }

    /// The occurrences from `from` to `to`, both days included.
    pub fn events(
        &self,
        account: &str,
        id: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Value>> {
        let (from, to) = (from.to_string(), to.to_string());
        let mut v = self.run(&[
            "-a", account, "event", "list", "-k", id, "--from", &from, "--to", &to, "-s", "0",
        ])?;
        match v["events"].take() {
            Value::Array(events) => Ok(events),
            _ => bail!("calendula event list: no events in the output"),
        }
    }
}

/// Every configured account's calendars, labelled and coloured.
pub fn catalog(calendula: &Calendula, settings: &Settings) -> Result<Vec<Calendar>> {
    let accounts = if settings.accounts.is_empty() {
        calendula.accounts()?
    } else {
        settings.accounts.keys().cloned().collect()
    };
    let mut out = Vec::new();
    for account in accounts {
        let config = settings.accounts.get(&account);
        for listed in calendula.calendars(&account)? {
            let own = config.and_then(|c| c.calendars.get(&listed.name));
            let shown = config.is_none_or(|c| c.shown.is_empty() || c.shown.contains(&listed.name));
            let color = own
                .and_then(|o| o.color.clone())
                .or(listed.color.clone())
                .unwrap_or_else(|| PALETTE[out.len() % PALETTE.len()].to_string());
            out.push(Calendar {
                key: format!("{account}:{}", listed.id),
                label: own
                    .and_then(|o| o.label.clone())
                    .unwrap_or_else(|| listed.name.clone()),
                account: account.clone(),
                id: listed.id,
                name: listed.name,
                color,
                shown,
            });
        }
    }
    Ok(out)
}

/// Whether to show an occurrence: not when it was cancelled, nor when one of
/// `me` declined it.
pub fn keep(event: &Value, me: &[String]) -> bool {
    if event["status"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED"))
    {
        return false;
    }
    let declined = event["attendees"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|a| {
            let mine = a["email"]
                .as_str()
                .is_some_and(|e| me.iter().any(|m| m.eq_ignore_ascii_case(e)));
            mine && a["partstat"]
                .as_str()
                .is_some_and(|p| p.eq_ignore_ascii_case("DECLINED"))
        });
    !declined
}

/// Orders by start, an all-day event before what is timed that day.
/// calendula writes an all-day start as `YYYYMMDD` and a timed one as
/// RFC 3339; the day is taken in local time.
pub fn sort_key(event: &Value) -> i64 {
    if event["allDay"].as_bool().unwrap_or(false) {
        let day = event["start"].as_str().unwrap_or_default();
        return NaiveDate::parse_from_str(day.get(..8).unwrap_or(day), "%Y%m%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .and_then(|t| t.and_local_timezone(chrono::Local).earliest())
            .map_or(i64::MIN, |t| t.timestamp() - 1);
    }
    event["startsAt"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map_or(i64::MIN, |t| t.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn me() -> Vec<String> {
        vec!["Me@Example.com".into()]
    }

    #[test]
    fn drops_cancelled() {
        assert!(!keep(&json!({"status": "CANCELLED"}), &me()));
        assert!(keep(&json!({"status": "CONFIRMED"}), &me()));
        assert!(keep(&json!({"status": null}), &me()));
    }

    #[test]
    fn drops_what_i_declined() {
        let declined = json!({"attendees": [
            {"email": "me@example.com", "partstat": "DECLINED"},
        ]});
        assert!(!keep(&declined, &me()));
        let someone_else = json!({"attendees": [
            {"email": "other@example.com", "partstat": "DECLINED"},
            {"email": "me@example.com", "partstat": "ACCEPTED"},
        ]});
        assert!(keep(&someone_else, &me()));
    }

    #[test]
    fn sorts_by_instant_all_day_first() {
        let mut events = [
            json!({"allDay": false, "startsAt": "2026-11-16T10:00:00+01:00"}),
            json!({"allDay": false, "startsAt": "2026-11-16T08:30:00Z"}),
            json!({"allDay": true, "start": "20261116"}),
        ];
        events.sort_by_key(sort_key);
        assert_eq!(events[0]["allDay"], true);
        assert_eq!(events[1]["startsAt"], "2026-11-16T08:30:00Z");
    }
}
