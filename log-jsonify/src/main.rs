use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn jsonify_line(line: &str) -> String {
    match serde_json::from_str::<Value>(line) {
        Ok(_) => line.to_string(),
        Err(_) => json!({ "message": line }).to_string(),
    }
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line in stdin.lock().lines() {
        match line {
            Ok(line_content) => {
                writeln!(stdout_lock, "{}", jsonify_line(&line_content)).unwrap();
            }
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn passes_through_valid_json_object() {
        let line = r#"{"level":"info","msg":"hello"}"#;
        assert_eq!(jsonify_line(line), line);
    }

    #[test]
    fn passes_through_valid_json_array() {
        let line = r#"[1,2,3]"#;
        assert_eq!(jsonify_line(line), line);
    }

    #[test]
    fn wraps_plain_text_in_message_envelope() {
        let out = jsonify_line("just a log line");
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["message"], "just a log line");
    }

    #[test]
    fn wraps_empty_string() {
        let out = jsonify_line("");
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["message"], "");
    }

    #[test]
    fn wraps_strings_that_look_like_partial_json() {
        // A bare scalar `42` parses as valid JSON, so it passes through.
        assert_eq!(jsonify_line("42"), "42");
        // But unbalanced braces should be wrapped.
        let out = jsonify_line("{not json");
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["message"], "{not json");
    }

    #[test]
    fn preserves_quotes_in_wrapped_text() {
        let out = jsonify_line(r#"contains "quotes""#);
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["message"], r#"contains "quotes""#);
    }
}
