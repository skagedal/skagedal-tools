//! Build and parse AWS CloudWatch Logs Insights console URLs.
//!
//! The Console encodes complex values into the URL fragment using a Rison
//! variant. There is no first-party documentation for this format; the rules
//! below were reverse-engineered from links produced by the Console.
//!
//!   - Objects:  `(key1~value1~key2~value2~...)`
//!   - Arrays:   `(~value1~value2~...)`
//!   - Strings:  `'<encoded>` — opening quote, no closing quote;
//!     next `~` or `)` terminates the value.
//!   - Numbers:  bare decimal, no quoting.
//!   - In strings, every byte outside `[A-Za-z0-9_.-]` is encoded as `*XX`,
//!     where `XX` is the lower-case hex of the UTF-8 byte.
//!   - The fragment uses `$XX` (not `%XX`) to encode `?` and `=`, so the
//!     hash router can find its inner query string.

use indexmap::IndexMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum RisonValue {
    String(String),
    Number(f64),
    Array(Vec<RisonValue>),
    Object(IndexMap<String, RisonValue>),
}

impl RisonValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RisonValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            RisonValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[RisonValue]> {
        match self {
            RisonValue::Array(a) => Some(a),
            _ => None,
        }
    }
}

fn is_safe_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}

/// Encode `s` for use as a string value inside the Rison-like fragment format.
pub fn encode_aws_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if is_safe_byte(b) {
            out.push(b as char);
        } else {
            out.push('*');
            out.push_str(&format!("{:02x}", b));
        }
    }
    out
}

/// Inverse of [`encode_aws_string`].
pub fn decode_aws_string(encoded: &str) -> Result<String, ConsoleLinkError> {
    let bytes = encoded.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'*' {
            if i + 2 >= bytes.len() {
                return Err(ConsoleLinkError::Decode(format!(
                    "invalid *XX escape at position {i}: truncated"
                )));
            }
            let hex = &encoded[i + 1..i + 3];
            let byte = u8::from_str_radix(hex, 16).map_err(|_| {
                ConsoleLinkError::Decode(format!("invalid *XX escape at position {i}: *{hex}"))
            })?;
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|e| ConsoleLinkError::Decode(e.to_string()))
}

/// Encode a Rison value into AWS's flavor.
pub fn encode_rison(value: &RisonValue) -> String {
    match value {
        RisonValue::String(s) => format!("'{}", encode_aws_string(s)),
        RisonValue::Number(n) => format_number(*n),
        RisonValue::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(encode_rison).collect();
            format!("(~{})", parts.join("~"))
        }
        RisonValue::Object(obj) => {
            let parts: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}~{}", encode_aws_string(k), encode_rison(v)))
                .collect();
            format!("({})", parts.join("~"))
        }
    }
}

fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

#[derive(Debug, Error)]
pub enum ConsoleLinkError {
    #[error("{0}")]
    Decode(String),
    #[error("{0}")]
    Parse(String),
}

/// Inverse of [`encode_rison`].
pub fn decode_rison(input: &str) -> Result<RisonValue, ConsoleLinkError> {
    let mut parser = RisonParser { src: input.as_bytes(), pos: 0 };
    let value = parser.parse_value()?;
    if parser.pos != parser.src.len() {
        return Err(ConsoleLinkError::Decode(format!(
            "unexpected trailing characters at position {}: {}",
            parser.pos,
            std::str::from_utf8(&parser.src[parser.pos..]).unwrap_or("<invalid utf8>"),
        )));
    }
    Ok(value)
}

struct RisonParser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> RisonParser<'a> {
    fn parse_value(&mut self) -> Result<RisonValue, ConsoleLinkError> {
        let c = self.peek().ok_or_else(|| ConsoleLinkError::Decode("unexpected end of input".into()))?;
        if c == b'(' {
            self.pos += 1;
            if self.peek() == Some(b'~') {
                return Ok(RisonValue::Array(self.parse_array_body()?));
            }
            return Ok(RisonValue::Object(self.parse_object_body()?));
        }
        if c == b'\'' {
            self.pos += 1;
            let raw = self.read_until_delimiter();
            return Ok(RisonValue::String(decode_aws_string(raw)?));
        }
        self.parse_number()
    }

    fn parse_array_body(&mut self) -> Result<Vec<RisonValue>, ConsoleLinkError> {
        let mut out = Vec::new();
        while self.peek() == Some(b'~') {
            self.pos += 1;
            if self.peek() == Some(b')') {
                break;
            }
            out.push(self.parse_value()?);
        }
        if self.peek() != Some(b')') {
            return Err(ConsoleLinkError::Decode(format!(
                "expected ')' at position {}",
                self.pos
            )));
        }
        self.pos += 1;
        Ok(out)
    }

    fn parse_object_body(&mut self) -> Result<IndexMap<String, RisonValue>, ConsoleLinkError> {
        let mut out = IndexMap::new();
        if self.peek() == Some(b')') {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            let key = decode_aws_string(self.read_until_delimiter())?;
            if self.peek() != Some(b'~') {
                return Err(ConsoleLinkError::Decode(format!(
                    "expected '~' after key '{key}' at position {}",
                    self.pos
                )));
            }
            self.pos += 1;
            out.insert(key, self.parse_value()?);
            match self.peek() {
                Some(b'~') => {
                    self.pos += 1;
                    continue;
                }
                Some(b')') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {
                    return Err(ConsoleLinkError::Decode(format!(
                        "expected '~' or ')' at position {}",
                        self.pos
                    )));
                }
            }
        }
    }

    fn read_until_delimiter(&mut self) -> &'a str {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c == b'~' || c == b')' {
                break;
            }
            self.pos += 1;
        }
        std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("")
    }

    fn parse_number(&mut self) -> Result<RisonValue, ConsoleLinkError> {
        let start = self.pos;
        let raw = self.read_until_delimiter();
        if raw.is_empty() {
            return Err(ConsoleLinkError::Decode(format!(
                "invalid number '' at position {start}"
            )));
        }
        let n: f64 = raw.parse().map_err(|_| {
            ConsoleLinkError::Decode(format!("invalid number '{raw}' at position {start}"))
        })?;
        if !n.is_finite() {
            return Err(ConsoleLinkError::Decode(format!(
                "invalid number '{raw}' at position {start}"
            )));
        }
        Ok(RisonValue::Number(n))
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }
}

/// `kind: relative` (seconds before "now") or `kind: absolute` (epoch ms).
#[derive(Debug, Clone, Copy)]
pub enum TimeSpec {
    Relative { seconds_back: i64 },
    Absolute { start_ms: i64, end_ms: i64 },
}

#[derive(Debug)]
pub struct QueryDetailInput<'a> {
    pub query: &'a str,
    /// Full ARNs, e.g. `arn:aws:logs:eu-north-1:123:log-group:/foo/bar`.
    pub log_group_arns: &'a [String],
    pub time: TimeSpec,
    /// Optional saved-query UUID; AWS Console will assign one if omitted.
    pub query_id: Option<&'a str>,
    pub log_class: Option<&'a str>,
}

/// Build the queryDetail object the Console expects, in field order matching
/// Console-generated URLs.
pub fn build_query_detail(input: &QueryDetailInput<'_>) -> IndexMap<String, RisonValue> {
    let mut detail: IndexMap<String, RisonValue> = IndexMap::new();
    match input.time {
        TimeSpec::Relative { seconds_back } => {
            detail.insert("end".into(), RisonValue::Number(0.0));
            detail.insert("start".into(), RisonValue::Number(-(seconds_back as f64)));
            detail.insert("timeType".into(), RisonValue::String("RELATIVE".into()));
            detail.insert("tz".into(), RisonValue::String("UTC".into()));
            detail.insert("unit".into(), RisonValue::String("seconds".into()));
        }
        TimeSpec::Absolute { start_ms, end_ms } => {
            detail.insert("end".into(), RisonValue::Number(end_ms as f64));
            detail.insert("start".into(), RisonValue::Number(start_ms as f64));
            detail.insert("timeType".into(), RisonValue::String("ABSOLUTE".into()));
            detail.insert("tz".into(), RisonValue::String("UTC".into()));
            detail.insert("unit".into(), RisonValue::String("milliseconds".into()));
        }
    }
    detail.insert("editorString".into(), RisonValue::String(input.query.into()));
    if let Some(qid) = input.query_id {
        detail.insert("queryId".into(), RisonValue::String(qid.into()));
    }
    detail.insert(
        "source".into(),
        RisonValue::Array(
            input
                .log_group_arns
                .iter()
                .map(|s| RisonValue::String(s.clone()))
                .collect(),
        ),
    );
    detail.insert("lang".into(), RisonValue::String("CWLI".into()));
    detail.insert(
        "logClass".into(),
        RisonValue::String(input.log_class.unwrap_or("STANDARD").into()),
    );
    detail.insert("queryBy".into(), RisonValue::String("logGroupName".into()));
    detail
}

#[derive(Debug)]
pub struct ConsoleLinkInput<'a> {
    pub region: &'a str,
    pub query_detail: &'a IndexMap<String, RisonValue>,
}

/// Build the final shareable URL.
pub fn build_console_link(input: &ConsoleLinkInput<'_>) -> String {
    let inner = encode_rison(&RisonValue::Object(input.query_detail.clone()));
    format!(
        "https://{region}.console.aws.amazon.com/cloudwatch/home?region={region}#logsV2:logs-insights$3FqueryDetail$3D~{inner}",
        region = input.region,
    )
}

/// Build the standard CloudWatch Logs ARN for a log group.
pub fn log_group_arn(region: &str, account_id: &str, log_group_name: &str) -> String {
    format!("arn:aws:logs:{region}:{account_id}:log-group:{log_group_name}")
}

/// Parse a log-group ARN of the form
/// `arn:aws:logs:<region>:<account>:log-group:<name>` (with an optional
/// trailing `:*` selector). Returns None if the input doesn't match.
pub fn parse_log_group_arn(arn: &str) -> Option<(String, String, String)> {
    let prefix = "arn:aws:logs:";
    let rest = arn.strip_prefix(prefix)?;
    let mut parts = rest.splitn(4, ':');
    let region = parts.next()?.to_string();
    let account = parts.next()?.to_string();
    let kind = parts.next()?;
    if kind != "log-group" {
        return None;
    }
    let mut name = parts.next()?.to_string();
    if let Some(stripped) = name.strip_suffix(":*") {
        name = stripped.to_string();
    }
    if region.is_empty() || account.is_empty() || name.is_empty() {
        return None;
    }
    Some((region, account, name))
}

#[derive(Debug)]
pub struct ParsedConsoleLink {
    pub region: String,
    pub query_detail: IndexMap<String, RisonValue>,
}

/// Inverse of [`build_console_link`]. Accepts URLs whose fragment uses either
/// AWS's `$3F`/`$3D` encoding or standard `%3F`/`%3D`.
pub fn parse_console_link(url: &str) -> Result<ParsedConsoleLink, ConsoleLinkError> {
    let trimmed = url.trim();
    let hash_idx = trimmed
        .find('#')
        .ok_or_else(|| ConsoleLinkError::Parse("URL has no fragment (no '#')".into()))?;
    let query_str = &trimmed[..hash_idx];
    let fragment = &trimmed[hash_idx + 1..];

    let region = extract_region(query_str)
        .ok_or_else(|| ConsoleLinkError::Parse("URL is missing the ?region=... query parameter".into()))?;

    let mut normalized = percent_decode_lossy(fragment);
    normalized = normalized.replace("$3F", "?").replace("$3D", "=");

    let detail_str = extract_query_detail(&normalized)
        .ok_or_else(|| ConsoleLinkError::Parse("fragment is missing queryDetail=...".into()))?;
    let detail = decode_rison(&detail_str)?;
    let RisonValue::Object(map) = detail else {
        return Err(ConsoleLinkError::Parse("queryDetail is not a Rison object".into()));
    };
    Ok(ParsedConsoleLink {
        region,
        query_detail: map,
    })
}

fn extract_region(query_str: &str) -> Option<String> {
    let q_start = query_str.find('?')?;
    let q = &query_str[q_start + 1..];
    for pair in q.split('&') {
        if let Some(rest) = pair.strip_prefix("region=") {
            return Some(percent_decode_lossy(rest));
        }
    }
    None
}

fn extract_query_detail(s: &str) -> Option<String> {
    // Look for `queryDetail=` prefixed by either start, `?`, or `&`.
    let key = "queryDetail=";
    for (i, _) in s.match_indices(key) {
        let preceded_ok = i == 0 || matches!(s.as_bytes()[i - 1], b'?' | b'&');
        if !preceded_ok {
            continue;
        }
        let mut after = &s[i + key.len()..];
        if let Some(stripped) = after.strip_prefix('~') {
            after = stripped;
        }
        return Some(after.to_string());
    }
    None
}

fn percent_decode_lossy(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> RisonValue {
        RisonValue::String(v.into())
    }
    fn n(v: f64) -> RisonValue {
        RisonValue::Number(v)
    }

    #[test]
    fn encode_aws_leaves_safe_alone() {
        assert_eq!(encode_aws_string("abcXYZ_0123.-"), "abcXYZ_0123.-");
    }

    #[test]
    fn encode_aws_hex_escapes_other_ascii() {
        assert_eq!(encode_aws_string(" "), "*20");
        assert_eq!(encode_aws_string("@"), "*40");
        assert_eq!(encode_aws_string(","), "*2c");
        assert_eq!(encode_aws_string("\n"), "*0a");
        assert_eq!(encode_aws_string("|"), "*7c");
        assert_eq!(encode_aws_string("="), "*3d");
        assert_eq!(encode_aws_string("'"), "*27");
        assert_eq!(encode_aws_string(":"), "*3a");
        assert_eq!(encode_aws_string("/"), "*2f");
        assert_eq!(encode_aws_string("~"), "*7e");
        assert_eq!(encode_aws_string("("), "*28");
        assert_eq!(encode_aws_string(")"), "*29");
        assert_eq!(encode_aws_string("*"), "*2a");
    }

    #[test]
    fn encode_aws_utf8_per_byte() {
        assert_eq!(encode_aws_string("é"), "*c3*a9");
        assert_eq!(encode_aws_string("✓"), "*e2*9c*93");
    }

    #[test]
    fn encode_rison_basics() {
        assert_eq!(encode_rison(&n(0.0)), "0");
        assert_eq!(encode_rison(&n(-3600.0)), "-3600");
        assert_eq!(encode_rison(&s("UTC")), "'UTC");
        assert_eq!(
            encode_rison(&RisonValue::Array(vec![s("a"), s("b")])),
            "(~'a~'b)"
        );
        let mut obj = IndexMap::new();
        obj.insert("end".into(), n(0.0));
        obj.insert("start".into(), n(-3600.0));
        obj.insert("tz".into(), s("UTC"));
        assert_eq!(
            encode_rison(&RisonValue::Object(obj)),
            "(end~0~start~-3600~tz~'UTC)"
        );
    }

    #[test]
    fn encode_rison_preserves_key_order() {
        let mut obj = IndexMap::new();
        obj.insert("z".into(), n(1.0));
        obj.insert("a".into(), n(2.0));
        obj.insert("m".into(), n(3.0));
        assert_eq!(encode_rison(&RisonValue::Object(obj)), "(z~1~a~2~m~3)");
    }

    #[test]
    fn log_group_arn_basic() {
        assert_eq!(
            log_group_arn("eu-north-1", "123456789012", "/my/service/logs"),
            "arn:aws:logs:eu-north-1:123456789012:log-group:/my/service/logs"
        );
    }

    #[test]
    fn build_console_link_byte_for_byte() {
        let expected = concat!(
            "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1",
            "#logsV2:logs-insights$3FqueryDetail$3D~(end~0~start~-3600~timeType~'RELATIVE",
            "~tz~'UTC~unit~'seconds~editorString~'fields*20*40timestamp*2c*20*40message",
            "*0a*7c*20sort*20*40timestamp*20desc*0a*7c*20filter*20app*20*3d*20",
            "*27my-service*27*0a*7c*20limit*20200",
            "~queryId~'dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
            "~source~(~'arn*3aaws*3alogs*3aeu-north-1*3a123456789012*3alog-group",
            "*3a*2fmy*2fservice*2flogs)",
            "~lang~'CWLI~logClass~'STANDARD~queryBy~'logGroupName)"
        );

        let mut detail = IndexMap::new();
        detail.insert("end".into(), n(0.0));
        detail.insert("start".into(), n(-3600.0));
        detail.insert("timeType".into(), s("RELATIVE"));
        detail.insert("tz".into(), s("UTC"));
        detail.insert("unit".into(), s("seconds"));
        detail.insert(
            "editorString".into(),
            s("fields @timestamp, @message\n| sort @timestamp desc\n| filter app = 'my-service'\n| limit 200"),
        );
        detail.insert(
            "queryId".into(),
            s("dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb"),
        );
        detail.insert(
            "source".into(),
            RisonValue::Array(vec![s("arn:aws:logs:eu-north-1:123456789012:log-group:/my/service/logs")]),
        );
        detail.insert("lang".into(), s("CWLI"));
        detail.insert("logClass".into(), s("STANDARD"));
        detail.insert("queryBy".into(), s("logGroupName"));

        assert_eq!(
            build_console_link(&ConsoleLinkInput {
                region: "eu-north-1",
                query_detail: &detail,
            }),
            expected
        );
    }

    #[test]
    fn build_query_detail_relative() {
        let arns = vec!["arn:aws:logs:eu-north-1:1:log-group:/x".to_string()];
        let detail = build_query_detail(&QueryDetailInput {
            query: "fields @timestamp",
            log_group_arns: &arns,
            time: TimeSpec::Relative { seconds_back: 3600 },
            query_id: None,
            log_class: None,
        });
        assert_eq!(detail.get("end").unwrap().as_f64(), Some(0.0));
        assert_eq!(detail.get("start").unwrap().as_f64(), Some(-3600.0));
        assert_eq!(detail.get("timeType").unwrap().as_str(), Some("RELATIVE"));
        assert_eq!(detail.get("unit").unwrap().as_str(), Some("seconds"));
        assert_eq!(detail.get("editorString").unwrap().as_str(), Some("fields @timestamp"));
        assert_eq!(
            detail.get("source").unwrap().as_array().unwrap()[0].as_str(),
            Some("arn:aws:logs:eu-north-1:1:log-group:/x")
        );
        assert_eq!(detail.get("lang").unwrap().as_str(), Some("CWLI"));
        assert_eq!(detail.get("logClass").unwrap().as_str(), Some("STANDARD"));
        assert_eq!(detail.get("queryBy").unwrap().as_str(), Some("logGroupName"));
        assert!(!detail.contains_key("queryId"));
    }

    #[test]
    fn build_query_detail_absolute() {
        let arns = vec!["arn:aws:logs:eu-north-1:1:log-group:/x".to_string()];
        let detail = build_query_detail(&QueryDetailInput {
            query: "fields @timestamp",
            log_group_arns: &arns,
            time: TimeSpec::Absolute {
                start_ms: 1700000000000,
                end_ms: 1700003600000,
            },
            query_id: None,
            log_class: None,
        });
        assert_eq!(detail.get("start").unwrap().as_f64(), Some(1700000000000.0));
        assert_eq!(detail.get("end").unwrap().as_f64(), Some(1700003600000.0));
        assert_eq!(detail.get("timeType").unwrap().as_str(), Some("ABSOLUTE"));
        assert_eq!(detail.get("unit").unwrap().as_str(), Some("milliseconds"));
    }

    #[test]
    fn build_query_detail_field_order() {
        let arns = vec!["arn:aws:logs:eu-north-1:1:log-group:/a".to_string()];
        let detail = build_query_detail(&QueryDetailInput {
            query: "q",
            log_group_arns: &arns,
            time: TimeSpec::Relative { seconds_back: 60 },
            query_id: None,
            log_class: None,
        });
        let keys: Vec<&str> = detail.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "end", "start", "timeType", "tz", "unit", "editorString", "source", "lang",
                "logClass", "queryBy"
            ]
        );
    }

    #[test]
    fn round_trip_string_array_with_specials() {
        assert_eq!(
            encode_rison(&RisonValue::Array(vec![s("a:b"), s("c/d")])),
            "(~'a*3ab~'c*2fd)"
        );
    }

    #[test]
    fn decode_aws_string_round_trip() {
        for value in [
            "abc",
            " ",
            "@",
            "@timestamp",
            "fields @timestamp, @message\n| sort @timestamp desc",
            "é",
            "✓",
            "*~()",
        ] {
            assert_eq!(decode_aws_string(&encode_aws_string(value)).unwrap(), value);
        }
    }

    #[test]
    fn decode_aws_string_rejects_malformed() {
        assert!(decode_aws_string("*").is_err());
        assert!(decode_aws_string("*g0").is_err());
    }

    #[test]
    fn decode_rison_round_trip() {
        let values: Vec<RisonValue> = vec![
            n(0.0),
            n(-3600.0),
            s("UTC"),
            s("fields @timestamp, @message\n| limit 200"),
            RisonValue::Array(vec![]),
            RisonValue::Array(vec![s("a"), s("b")]),
        ];
        for v in values {
            assert_eq!(decode_rison(&encode_rison(&v)).unwrap(), v);
        }
    }

    #[test]
    fn decode_rison_rejects_trailing_garbage() {
        assert!(decode_rison("(a~1)x").is_err());
    }

    #[test]
    fn parse_log_group_arn_basic() {
        let parsed =
            parse_log_group_arn("arn:aws:logs:eu-north-1:123456789012:log-group:/my/service/logs")
                .unwrap();
        assert_eq!(parsed.0, "eu-north-1");
        assert_eq!(parsed.1, "123456789012");
        assert_eq!(parsed.2, "/my/service/logs");
        assert!(parse_log_group_arn("not-an-arn").is_none());
        assert!(parse_log_group_arn("arn:aws:s3:::my-bucket").is_none());
    }

    #[test]
    fn parse_log_group_arn_strips_trailing_selector() {
        let parsed = parse_log_group_arn("arn:aws:logs:eu-north-1:1:log-group:/x:*").unwrap();
        assert_eq!(parsed, ("eu-north-1".into(), "1".into(), "/x".into()));
    }

    fn make_full_detail() -> IndexMap<String, RisonValue> {
        let mut detail = IndexMap::new();
        detail.insert("end".into(), n(0.0));
        detail.insert("start".into(), n(-3600.0));
        detail.insert("timeType".into(), s("RELATIVE"));
        detail.insert("tz".into(), s("UTC"));
        detail.insert("unit".into(), s("seconds"));
        detail.insert(
            "editorString".into(),
            s("fields @timestamp, @message\n| sort @timestamp desc\n| filter app = 'my-service'\n| limit 200"),
        );
        detail.insert("queryId".into(), s("dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb"));
        detail.insert(
            "source".into(),
            RisonValue::Array(vec![s("arn:aws:logs:eu-north-1:123456789012:log-group:/my/service/logs")]),
        );
        detail.insert("lang".into(), s("CWLI"));
        detail.insert("logClass".into(), s("STANDARD"));
        detail.insert("queryBy".into(), s("logGroupName"));
        detail
    }

    #[test]
    fn parse_console_link_round_trip() {
        let detail = make_full_detail();
        let url = build_console_link(&ConsoleLinkInput {
            region: "eu-north-1",
            query_detail: &detail,
        });
        let parsed = parse_console_link(&url).unwrap();
        assert_eq!(parsed.region, "eu-north-1");
        assert_eq!(parsed.query_detail, detail);
    }

    #[test]
    fn parse_console_link_tolerates_percent_encoding() {
        let mut detail = IndexMap::new();
        detail.insert("end".into(), n(0.0));
        detail.insert("start".into(), n(-60.0));
        detail.insert("timeType".into(), s("RELATIVE"));
        detail.insert("tz".into(), s("UTC"));
        detail.insert("unit".into(), s("seconds"));
        detail.insert("editorString".into(), s("fields @timestamp"));
        detail.insert(
            "source".into(),
            RisonValue::Array(vec![s("arn:aws:logs:eu-north-1:1:log-group:/x")]),
        );
        detail.insert("lang".into(), s("CWLI"));
        detail.insert("logClass".into(), s("STANDARD"));
        detail.insert("queryBy".into(), s("logGroupName"));

        let original = build_console_link(&ConsoleLinkInput {
            region: "eu-north-1",
            query_detail: &detail,
        });
        let percent = original.replacen("$3F", "%3F", 1).replacen("$3D", "%3D", 1);
        let parsed = parse_console_link(&percent).unwrap();
        assert_eq!(parsed.region, "eu-north-1");
        assert_eq!(parsed.query_detail, detail);
    }

    #[test]
    fn parse_console_link_rejects_missing_query_detail() {
        let url = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1#logsV2:logs-insights";
        let err = parse_console_link(url).unwrap_err();
        assert!(format!("{err}").contains("queryDetail"));
    }
}
