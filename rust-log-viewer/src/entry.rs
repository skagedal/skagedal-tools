use serde_json::{Map, Value};

/// A parsed log entry. Non-JSON input lines are wrapped under [`DEFAULT_FIELD`].
#[derive(Debug, Clone)]
pub struct Entry {
    pub raw: String,
    pub value: Value,
}

pub const DEFAULT_FIELD: &str = "message";

impl Entry {
    pub fn parse(line: &str) -> Self {
        let value = match serde_json::from_str::<Value>(line) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                let mut map = Map::new();
                map.insert(DEFAULT_FIELD.to_string(), Value::String(line.to_string()));
                Value::Object(map)
            }
        };
        Self {
            raw: line.to_string(),
            value,
        }
    }

    /// First non-empty value among the candidate keys, returned as a display string.
    pub fn pick(&self, candidates: &[&str]) -> String {
        let Value::Object(map) = &self.value else {
            return String::new();
        };
        for key in candidates {
            if let Some(v) = map.get(*key) {
                let s = stringify(v);
                if !s.is_empty() {
                    return s;
                }
            }
        }
        String::new()
    }
}

fn stringify(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object() {
        let e = Entry::parse(r#"{"level":"info","msg":"hi"}"#);
        assert_eq!(e.pick(&["level"]), "info");
        assert_eq!(e.pick(&["msg"]), "hi");
    }

    #[test]
    fn wraps_non_json_under_default_field() {
        let e = Entry::parse("plain text");
        assert_eq!(e.pick(&[DEFAULT_FIELD]), "plain text");
    }

    #[test]
    fn wraps_bare_scalars_too() {
        // log-viewer wraps bare scalars (numbers, strings) under the default
        // field — only objects pass through as structured entries.
        let e = Entry::parse("42");
        assert_eq!(e.pick(&[DEFAULT_FIELD]), "42");
    }

    #[test]
    fn pick_falls_through_candidates() {
        let e = Entry::parse(r#"{"timestamp":"2026-01-01"}"#);
        assert_eq!(e.pick(&["@timestamp", "timestamp", "ts"]), "2026-01-01");
    }

    #[test]
    fn pick_returns_empty_when_no_match() {
        let e = Entry::parse(r#"{"a":1}"#);
        assert_eq!(e.pick(&["b", "c"]), "");
    }

    #[test]
    fn pick_skips_null_and_empty_string() {
        let e = Entry::parse(r#"{"a":null,"b":"","c":"yes"}"#);
        assert_eq!(e.pick(&["a", "b", "c"]), "yes");
    }
}
