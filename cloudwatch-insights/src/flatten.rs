//! Flatten JSON-encoded fields into the row.

use indexmap::IndexMap;
use serde_json::Value;

/// For each field in `fields_to_flatten`, parse its value as a JSON object
/// and merge the resulting keys into the row. Original field is removed.
/// Invalid-JSON or non-object values are left untouched.
pub fn flatten_row(
    row: &IndexMap<String, String>,
    fields_to_flatten: &[String],
) -> IndexMap<String, Value> {
    let mut out: IndexMap<String, Value> = row
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();

    if fields_to_flatten.is_empty() {
        return out;
    }

    for field in fields_to_flatten {
        let raw = match out.get(field) {
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Value::Object(map) = parsed else {
            continue;
        };
        out.shift_remove(field);
        for (k, v) in map {
            out.insert(k, v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn flattens_json_field() {
        let r = row(&[
            ("@timestamp", "2026-04-25T12:00:00Z"),
            ("@message", r#"{"foo": "bar", "baz": "frotz"}"#),
        ]);
        let out = flatten_row(&r, &["@message".into()]);
        assert_eq!(out.get("@timestamp").unwrap().as_str().unwrap(), "2026-04-25T12:00:00Z");
        assert_eq!(out.get("foo").unwrap().as_str().unwrap(), "bar");
        assert_eq!(out.get("baz").unwrap().as_str().unwrap(), "frotz");
        assert!(!out.contains_key("@message"));
    }

    #[test]
    fn leaves_field_when_not_json() {
        let r = row(&[("@message", "not json at all")]);
        let out = flatten_row(&r, &["@message".into()]);
        assert_eq!(out.get("@message").unwrap().as_str().unwrap(), "not json at all");
    }

    #[test]
    fn leaves_field_when_json_not_object() {
        let r = row(&[("@message", r#"["a", "b"]"#)]);
        let out = flatten_row(&r, &["@message".into()]);
        assert_eq!(out.get("@message").unwrap().as_str().unwrap(), r#"["a", "b"]"#);
    }

    #[test]
    fn preserves_nested_values() {
        let r = row(&[("@message", r#"{"n": 1, "ok": true, "ctx": {"id": "x"}}"#)]);
        let out = flatten_row(&r, &["@message".into()]);
        assert_eq!(out.get("n").unwrap(), &Value::Number(1.into()));
        assert_eq!(out.get("ok").unwrap(), &Value::Bool(true));
        assert_eq!(
            out.get("ctx").unwrap(),
            &serde_json::json!({"id": "x"})
        );
    }

    #[test]
    fn empty_fields_returns_copy() {
        let r = row(&[("a", "1"), ("b", "2")]);
        let out = flatten_row(&r, &[]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn skips_absent_fields() {
        let r = row(&[("kept", "yes")]);
        let out = flatten_row(&r, &["@message".into()]);
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("kept").unwrap().as_str().unwrap(), "yes");
    }

    #[test]
    fn flattened_keys_overwrite_collisions() {
        let r = row(&[
            ("foo", "outer"),
            ("@message", r#"{"foo": "inner"}"#),
        ]);
        let out = flatten_row(&r, &["@message".into()]);
        assert_eq!(out.get("foo").unwrap().as_str().unwrap(), "inner");
        assert!(!out.contains_key("@message"));
    }
}
