use serde_json::{Map, Value};

/// A parsed log entry. Non-JSON input lines (and bare scalars) are wrapped
/// under the configured `default_field`.
#[derive(Debug, Clone)]
pub struct Entry {
    pub raw: String,
    pub value: Value,
}

impl Entry {
    pub fn parse(line: &str, default_field: &str) -> Self {
        let value = match serde_json::from_str::<Value>(line) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                let mut map = Map::new();
                map.insert(default_field.to_string(), Value::String(line.to_string()));
                Value::Object(map)
            }
        };
        Self {
            raw: line.to_string(),
            value,
        }
    }

    pub fn object(&self) -> Option<&Map<String, Value>> {
        match &self.value {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    /// First non-empty value among the candidate keys, returned as a display string.
    pub fn pick(&self, candidates: &[String]) -> String {
        let Some(map) = self.object() else {
            return String::new();
        };
        for key in candidates {
            if let Some(v) = map.get(key) {
                let s = stringify(v);
                if !s.is_empty() {
                    return s;
                }
            }
        }
        String::new()
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.object().and_then(|m| m.get(key)).map(stringify)
    }

    pub fn keys(&self) -> Vec<String> {
        self.object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }
}

pub fn stringify(v: &Value) -> String {
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

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn parses_object() {
        let e = Entry::parse(r#"{"level":"info","msg":"hi"}"#, "message");
        assert_eq!(e.pick(&s(&["level"])), "info");
        assert_eq!(e.pick(&s(&["msg"])), "hi");
    }

    #[test]
    fn wraps_non_json_under_default_field() {
        let e = Entry::parse("plain text", "message");
        assert_eq!(e.pick(&s(&["message"])), "plain text");
    }

    #[test]
    fn wraps_bare_scalars_too() {
        let e = Entry::parse("42", "message");
        assert_eq!(e.pick(&s(&["message"])), "42");
    }

    #[test]
    fn pick_falls_through_candidates() {
        let e = Entry::parse(r#"{"timestamp":"2026-01-01"}"#, "message");
        assert_eq!(
            e.pick(&s(&["@timestamp", "timestamp", "ts"])),
            "2026-01-01"
        );
    }

    #[test]
    fn pick_returns_empty_when_no_match() {
        let e = Entry::parse(r#"{"a":1}"#, "message");
        assert_eq!(e.pick(&s(&["b", "c"])), "");
    }

    #[test]
    fn pick_skips_null_and_empty_string() {
        let e = Entry::parse(r#"{"a":null,"b":"","c":"yes"}"#, "message");
        assert_eq!(e.pick(&s(&["a", "b", "c"])), "yes");
    }

    #[test]
    fn keys_preserves_insertion_order() {
        let e = Entry::parse(r#"{"z":1,"a":2,"m":3}"#, "message");
        assert_eq!(e.keys(), vec!["z", "a", "m"]);
    }

    #[test]
    fn get_str_for_complex_values() {
        let e = Entry::parse(r#"{"obj":{"k":1},"n":42}"#, "message");
        assert_eq!(e.get_str("n").unwrap(), "42");
        let obj_str = e.get_str("obj").unwrap();
        assert!(obj_str.contains("\"k\""));
    }
}
