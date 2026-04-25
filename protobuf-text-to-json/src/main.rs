use serde_json::{Map, Value};
use std::io::{self, Read};

/// A simple parser for protobuf text format
struct Parser {
    input: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Parser {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(ch) = self.peek() {
                if ch.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            // Skip line comments
            if self.peek() == Some('#') {
                while let Some(ch) = self.peek() {
                    self.advance();
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }

            break;
        }
    }

    fn parse_identifier(&mut self) -> Option<String> {
        self.skip_whitespace_and_comments();
        let mut ident = String::new();

        // First character must be letter or underscore
        if let Some(ch) = self.peek() {
            if ch.is_alphabetic() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                return None;
            }
        } else {
            return None;
        }

        // Subsequent characters can be alphanumeric, underscore, or dot (for extensions)
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' || ch == '.' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        Some(ident)
    }

    fn parse_string(&mut self) -> Option<String> {
        self.skip_whitespace_and_comments();
        let quote = self.peek()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        self.advance(); // consume opening quote

        let mut result = String::new();
        while let Some(ch) = self.advance() {
            if ch == quote {
                return Some(result);
            } else if ch == '\\' {
                // Handle escape sequences
                if let Some(escaped) = self.advance() {
                    match escaped {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        '\'' => result.push('\''),
                        'x' => {
                            // Hex escape \xNN
                            let mut hex = String::new();
                            for _ in 0..2 {
                                if let Some(h) = self.peek()
                                    && h.is_ascii_hexdigit()
                                {
                                    hex.push(h);
                                    self.advance();
                                }
                            }
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                result.push(byte as char);
                            }
                        }
                        _ => {
                            result.push(escaped);
                        }
                    }
                }
            } else {
                result.push(ch);
            }
        }
        None // Unterminated string
    }

    fn parse_number(&mut self) -> Option<Value> {
        self.skip_whitespace_and_comments();
        let mut num_str = String::new();
        let mut is_float = false;

        // Handle negative sign
        if self.peek() == Some('-') {
            num_str.push('-');
            self.advance();
        }

        // Check for special float values
        if self.peek() == Some('i') || self.peek() == Some('n') {
            let start = self.pos;
            if let Some(ident) = self.parse_identifier() {
                match ident.as_str() {
                    "inf" => return Some(Value::from(f64::INFINITY)),
                    "nan" => return Some(Value::from(f64::NAN)),
                    _ => {
                        self.pos = start;
                        return None;
                    }
                }
            }
        }

        // Parse digits before decimal point
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for hex number
        if num_str == "0" && self.peek() == Some('x') {
            num_str.push('x');
            self.advance();
            while let Some(ch) = self.peek() {
                if ch.is_ascii_hexdigit() {
                    num_str.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
            if let Ok(n) = i64::from_str_radix(&num_str[2..], 16) {
                return Some(Value::from(n));
            }
            return None;
        }

        // Check for decimal point
        if self.peek() == Some('.') {
            is_float = true;
            num_str.push('.');
            self.advance();
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    num_str.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        // Check for exponent
        if self.peek() == Some('e') || self.peek() == Some('E') {
            is_float = true;
            num_str.push('e');
            self.advance();
            if self.peek() == Some('+') || self.peek() == Some('-') {
                num_str.push(self.advance().unwrap());
            }
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    num_str.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if num_str.is_empty() || num_str == "-" {
            return None;
        }

        if is_float {
            num_str.parse::<f64>().ok().map(Value::from)
        } else {
            // Try to parse as i64 first, fall back to u64 for large positive numbers
            if let Ok(n) = num_str.parse::<i64>() {
                Some(Value::from(n))
            } else if let Ok(n) = num_str.parse::<u64>() {
                Some(Value::from(n))
            } else {
                None
            }
        }
    }

    fn parse_value(&mut self) -> Option<Value> {
        self.skip_whitespace_and_comments();

        match self.peek()? {
            '"' | '\'' => self.parse_string().map(Value::String),
            '{' | '<' => self.parse_message(),
            '[' => self.parse_array(),
            ch if ch.is_ascii_digit() || ch == '-' => self.parse_number(),
            ch if ch.is_alphabetic() || ch == '_' => {
                let ident = self.parse_identifier()?;
                match ident.as_str() {
                    "true" => Some(Value::Bool(true)),
                    "false" => Some(Value::Bool(false)),
                    "inf" => Some(Value::from(f64::INFINITY)),
                    "nan" => Some(Value::from(f64::NAN)),
                    _ => Some(Value::String(ident)), // Enum value
                }
            }
            _ => None,
        }
    }

    fn parse_array(&mut self) -> Option<Value> {
        self.skip_whitespace_and_comments();
        if self.peek() != Some('[') {
            return None;
        }
        self.advance(); // consume '['

        let mut arr = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if self.peek() == Some(']') {
                self.advance();
                break;
            }

            if let Some(value) = self.parse_value() {
                arr.push(value);
            } else {
                break;
            }

            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.advance();
            }
        }

        Some(Value::Array(arr))
    }

    fn parse_message(&mut self) -> Option<Value> {
        self.skip_whitespace_and_comments();

        let close_char = match self.peek()? {
            '{' => '}',
            '<' => '>',
            _ => return None,
        };
        self.advance(); // consume opening brace

        let mut fields: Map<String, Value> = Map::new();

        loop {
            self.skip_whitespace_and_comments();

            // Check for closing brace
            if self.peek() == Some(close_char) {
                self.advance();
                break;
            }

            // Check for end of input
            if self.peek().is_none() {
                break;
            }

            // Parse field name
            let field_name = self.parse_identifier()?;

            self.skip_whitespace_and_comments();

            // Check for nested message (without colon) or value (with colon)
            let value = match self.peek() {
                Some(':') => {
                    self.advance(); // consume ':'
                    self.skip_whitespace_and_comments();
                    self.parse_value()?
                }
                Some('{') | Some('<') => self.parse_message()?,
                _ => return None,
            };

            // Handle repeated fields by converting to array
            if let Some(existing) = fields.remove(&field_name) {
                let arr = match existing {
                    Value::Array(mut arr) => {
                        arr.push(value);
                        arr
                    }
                    other => vec![other, value],
                };
                fields.insert(field_name, Value::Array(arr));
            } else {
                fields.insert(field_name, value);
            }

            self.skip_whitespace_and_comments();

            // Optional comma or semicolon separator
            if self.peek() == Some(',') || self.peek() == Some(';') {
                self.advance();
            }
        }

        Some(Value::Object(fields))
    }

    fn parse_top_level(&mut self) -> Option<Value> {
        self.skip_whitespace_and_comments();

        // Check if the input starts with a brace (message literal)
        if self.peek() == Some('{') || self.peek() == Some('<') {
            return self.parse_message();
        }

        // Otherwise, parse as a message without braces (top-level fields)
        let mut fields: Map<String, Value> = Map::new();

        loop {
            self.skip_whitespace_and_comments();

            if self.peek().is_none() {
                break;
            }

            // Parse field name
            let field_name = match self.parse_identifier() {
                Some(name) => name,
                None => break,
            };

            self.skip_whitespace_and_comments();

            // Check for nested message (without colon) or value (with colon)
            let value = match self.peek() {
                Some(':') => {
                    self.advance(); // consume ':'
                    self.skip_whitespace_and_comments();
                    match self.parse_value() {
                        Some(v) => v,
                        None => break,
                    }
                }
                Some('{') | Some('<') => match self.parse_message() {
                    Some(v) => v,
                    None => break,
                },
                _ => break,
            };

            // Handle repeated fields by converting to array
            if let Some(existing) = fields.remove(&field_name) {
                let arr = match existing {
                    Value::Array(mut arr) => {
                        arr.push(value);
                        arr
                    }
                    other => vec![other, value],
                };
                fields.insert(field_name, Value::Array(arr));
            } else {
                fields.insert(field_name, value);
            }

            self.skip_whitespace_and_comments();

            // Optional comma or semicolon separator
            if self.peek() == Some(',') || self.peek() == Some(';') {
                self.advance();
            }
        }

        if fields.is_empty() {
            None
        } else {
            Some(Value::Object(fields))
        }
    }
}

fn main() {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("Error reading input: {}", e);
        std::process::exit(1);
    }

    let mut parser = Parser::new(&input);

    match parser.parse_top_level() {
        Some(value) => {
            if let Err(e) = serde_json::to_writer_pretty(io::stdout(), &value) {
                eprintln!("Error writing JSON: {}", e);
                std::process::exit(1);
            }
            println!(); // Add trailing newline
        }
        None => {
            eprintln!("Failed to parse protobuf text format");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(input: &str) -> Value {
        Parser::new(input)
            .parse_top_level()
            .unwrap_or_else(|| panic!("failed to parse: {input}"))
    }

    #[test]
    fn parses_simple_scalar_fields() {
        let got = parse(r#"name: "Alice" id: 7 active: true"#);
        assert_eq!(got, json!({"name": "Alice", "id": 7, "active": true}));
    }

    #[test]
    fn parses_nested_message_with_braces() {
        let got = parse(r#"point { x: 1 y: 2 }"#);
        assert_eq!(got, json!({"point": {"x": 1, "y": 2}}));
    }

    #[test]
    fn parses_nested_message_with_angle_brackets() {
        let got = parse(r#"point < x: 1 y: 2 >"#);
        assert_eq!(got, json!({"point": {"x": 1, "y": 2}}));
    }

    #[test]
    fn repeated_fields_become_array() {
        let got = parse(r#"tag: "a" tag: "b" tag: "c""#);
        assert_eq!(got, json!({"tag": ["a", "b", "c"]}));
    }

    #[test]
    fn repeated_messages_become_array() {
        let got = parse(r#"phone { number: "1" } phone { number: "2" }"#);
        assert_eq!(
            got,
            json!({"phone": [{"number": "1"}, {"number": "2"}]})
        );
    }

    #[test]
    fn array_literal_syntax() {
        let got = parse(r#"nums: [1, 2, 3]"#);
        assert_eq!(got, json!({"nums": [1, 2, 3]}));
    }

    #[test]
    fn enum_values_become_strings() {
        let got = parse(r#"type: HOME"#);
        assert_eq!(got, json!({"type": "HOME"}));
    }

    #[test]
    fn handles_floats_and_negatives() {
        let got = parse(r#"score: 98.5 delta: -3"#);
        assert_eq!(got, json!({"score": 98.5, "delta": -3}));
    }

    #[test]
    fn handles_hex_numbers() {
        let got = parse(r#"flags: 0xff"#);
        assert_eq!(got, json!({"flags": 255}));
    }

    #[test]
    fn handles_string_escapes() {
        let got = parse(r#"s: "line1\nline2\t!""#);
        assert_eq!(got, json!({"s": "line1\nline2\t!"}));
    }

    #[test]
    fn skips_line_comments() {
        let got = parse("# leading\nname: \"x\" # trailing\nid: 1\n");
        assert_eq!(got, json!({"name": "x", "id": 1}));
    }

    #[test]
    fn parses_optional_comma_and_semicolon_separators() {
        let got = parse(r#"a: 1, b: 2; c: 3"#);
        assert_eq!(got, json!({"a": 1, "b": 2, "c": 3}));
    }

    #[test]
    fn parses_full_example_file() {
        let input = include_str!("../example.textproto");
        let got = parse(input);
        assert_eq!(
            got,
            json!({
                "name": "John Doe",
                "id": 1234,
                "email": "jdoe@example.com",
                "phones": [
                    {"number": "555-4321", "type": "HOME"},
                    {"number": "555-1234", "type": "MOBILE"}
                ],
                "active": true,
                "score": 98.5,
                "metadata": {
                    "created_at": 1609459200,
                    "tags": ["important", "verified"]
                }
            })
        );
    }

    #[test]
    fn empty_input_yields_none() {
        assert!(Parser::new("").parse_top_level().is_none());
        assert!(Parser::new("   \n  ").parse_top_level().is_none());
    }

    #[test]
    fn top_level_braces_message() {
        let got = parse(r#"{ a: 1 b: 2 }"#);
        assert_eq!(got, json!({"a": 1, "b": 2}));
    }
}
