use std::collections::HashSet;
use std::process::Command;
use std::time::Instant;

use crate::config::TriggerDef;
use crate::entry::Entry;

pub struct TriggerRuntime {
    triggers: Vec<TriggerState>,
    started_at: Instant,
}

struct TriggerState {
    def: TriggerDef,
    seen: HashSet<String>,
}

impl TriggerRuntime {
    pub fn new(defs: Vec<TriggerDef>) -> Self {
        Self {
            triggers: defs
                .into_iter()
                .map(|def| TriggerState {
                    def,
                    seen: HashSet::new(),
                })
                .collect(),
            started_at: Instant::now(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.triggers.is_empty()
    }

    /// Process an entry: for each trigger, check if its watched key has a
    /// value we haven't seen before. If so, fire (unless we're still inside
    /// the startup-delay window, in which case we just record).
    pub fn handle(&mut self, entry: &Entry) {
        let in_startup =
            |delay: u64| self.started_at.elapsed().as_millis() < u128::from(delay);
        for state in &mut self.triggers {
            let value = pick_first(entry, &state.def.on_new_value);
            let Some(value) = value else { continue };
            if state.seen.contains(&value) {
                continue;
            }
            state.seen.insert(value.clone());
            if !in_startup(state.def.startup_delay_ms) {
                fire(&state.def, &value, entry);
            }
        }
    }
}

fn pick_first(entry: &Entry, candidates: &[String]) -> Option<String> {
    for key in candidates {
        if let Some(s) = entry.get_str(key)
            && !s.is_empty()
        {
            return Some(s);
        }
    }
    None
}

fn fire(def: &TriggerDef, value: &str, entry: &Entry) {
    let field = def.on_new_value.first().cloned().unwrap_or_default();
    let action = substitute(&def.action, value, &field);
    let entry_json = serde_json::to_string(&entry.value).unwrap_or_default();
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&action);
    cmd.env("LOG_VIEWER_VALUE", value);
    cmd.env("LOG_VIEWER_FIELD", &field);
    cmd.env("LOG_VIEWER_TRIGGER", &def.name);
    cmd.env("LOG_VIEWER_ENTRY", entry_json);
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

fn substitute(template: &str, value: &str, field: &str) -> String {
    let value_q = shell_escape::unix::escape(value.into());
    let field_q = shell_escape::unix::escape(field.into());
    template
        .replace("{value}", &value_q)
        .replace("{field}", &field_q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(action: &str, on: &[&str], delay: u64) -> TriggerDef {
        TriggerDef {
            name: "t".into(),
            on_new_value: on.iter().map(|s| (*s).to_string()).collect(),
            action: action.into(),
            startup_delay_ms: delay,
        }
    }

    #[test]
    fn substitution_quotes_special_chars() {
        let out = substitute("echo {value} on {field}", "hello world", "podname");
        assert_eq!(out, "echo 'hello world' on podname");
    }

    #[test]
    fn substitution_handles_quotes() {
        let out = substitute("echo {value}", "it's", "k");
        assert!(out.contains("'it'"));
    }

    #[test]
    fn pick_first_returns_first_non_empty() {
        let e = Entry::parse(r#"{"a":"","b":"v"}"#, "message");
        assert_eq!(
            pick_first(&e, &["a".into(), "b".into()]).as_deref(),
            Some("v")
        );
    }

    #[test]
    fn pick_first_returns_none_when_missing() {
        let e = Entry::parse(r#"{"a":1}"#, "message");
        assert_eq!(pick_first(&e, &["b".into()]), None);
    }

    #[test]
    fn handle_records_first_value_without_firing_during_startup() {
        let mut rt = TriggerRuntime::new(vec![def("true", &["pod"], 60_000)]);
        let e = Entry::parse(r#"{"pod":"p1"}"#, "message");
        rt.handle(&e);
        assert!(rt.triggers[0].seen.contains("p1"));
    }

    #[test]
    fn handle_dedupes_seen_values() {
        let mut rt = TriggerRuntime::new(vec![def("true", &["pod"], 60_000)]);
        rt.handle(&Entry::parse(r#"{"pod":"p1"}"#, "message"));
        rt.handle(&Entry::parse(r#"{"pod":"p1"}"#, "message"));
        assert_eq!(rt.triggers[0].seen.len(), 1);
    }
}
