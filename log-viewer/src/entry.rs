use serde_json::{Map, Value};

/// A parsed log entry. Non-JSON input lines (and bare scalars) are wrapped
/// under the configured `default_field`.
#[derive(Debug, Clone)]
pub struct Entry {
    pub raw: String,
    pub value: Value,
    /// True when the original line wasn't a JSON object and was wrapped
    /// under the default field. Mirrors the TS log-viewer's `wrapped` flag
    /// so the React browser app can render the same "(wrapped)" hint.
    /// Only the `web` front-end surfaces this; the TUI ignores it.
    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    pub wrapped: bool,
    /// True when this line came from the executed subprocess's stderr
    /// stream rather than its stdout. The TUI renders these in red.
    pub is_stderr: bool,
}

impl Entry {
    pub fn parse(line: &str, default_field: &str) -> Self {
        let (value, wrapped) = match serde_json::from_str::<Value>(line) {
            Ok(v @ Value::Object(_)) => (v, false),
            _ => {
                let mut map = Map::new();
                map.insert(default_field.to_string(), Value::String(line.to_string()));
                (Value::Object(map), true)
            }
        };
        Self {
            raw: line.to_string(),
            value,
            wrapped,
            is_stderr: false,
        }
    }

    pub fn parse_stderr(line: &str, default_field: &str) -> Self {
        let mut e = Self::parse(line, default_field);
        e.is_stderr = true;
        e
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

/// Subsequence (fuzzy) match: every char of `needle` must appear in
/// `haystack` in order, case-insensitively. Empty needle matches anything.
/// Mirrors the TS log-viewer's `fuzzyMatch`.
pub fn fuzzy_match(needle: &str, haystack: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle_lc = needle.to_lowercase();
    let haystack_lc = haystack.to_lowercase();
    let mut hay = haystack_lc.chars();
    'outer: for nc in needle_lc.chars() {
        for hc in hay.by_ref() {
            if hc == nc {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// Emacs-style C-w: drop the word to the left of `cursor` plus any whitespace
/// between the cursor and that word.
pub fn kill_word_left(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return (text.to_string(), 0);
    }
    let bytes = text.as_bytes();
    let mut i = cursor;
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    while i > 0 && !bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let mut out = String::with_capacity(text.len() - (cursor - i));
    out.push_str(&text[..i]);
    out.push_str(&text[cursor..]);
    (out, i)
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

    #[test]
    fn fuzzy_match_basic() {
        assert!(fuzzy_match("", "anything"));
        assert!(fuzzy_match("abc", "axbxc"));
        assert!(fuzzy_match("ABC", "aXbXcXd")); // case-insensitive
        assert!(!fuzzy_match("abc", "acb"));
        assert!(!fuzzy_match("abcd", "abc"));
    }

    #[test]
    fn fuzzy_match_finds_subsequence_in_jsonl() {
        assert!(fuzzy_match("err", "level=error msg=oops"));
        assert!(fuzzy_match("nginx", "podname=nginx-abc-1"));
    }

    #[test]
    fn kill_word_left_basics() {
        let (t, c) = kill_word_left("hello world", 11);
        assert_eq!(t, "hello ");
        assert_eq!(c, 6);
        let (t, c) = kill_word_left("hello world", 6);
        assert_eq!(t, "world");
        assert_eq!(c, 0);
        let (t, c) = kill_word_left("", 0);
        assert_eq!(t, "");
        assert_eq!(c, 0);
    }

    #[test]
    fn kill_word_left_keeps_text_after_cursor() {
        let (t, c) = kill_word_left("foo bar baz", 7);
        assert_eq!(t, "foo  baz");
        assert_eq!(c, 4);
    }
}
