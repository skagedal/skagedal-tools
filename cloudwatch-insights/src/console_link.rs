//! Build and parse AWS CloudWatch Logs Insights console URLs.
//!
//! There are two fragment routes in the wild, and parsing accepts both:
//!
//!   - `#logsV2:logs-insights?queryDetail=~(...)` — classic Logs Insights. One
//!     Rison object holding the query, the log group ARNs and the time window.
//!     This is also the format we build.
//!   - `#log-analytics?active=~'a&a.query=~'...` — the revamped console
//!     ("Log Analytics"). An ordinary query string whose values are Rison, one
//!     group of `<tab>.<field>` parameters per editor tab. The log groups and
//!     the time window are no longer separate fields: they are the `SOURCE ...
//!     START=... END=...` command at the head of the query text (see
//!     [`crate::source_command`]).
//!
//! Both routes encode values into the fragment using the same Rison variant.
//! There is no first-party documentation for this format; the rules below were
//! reverse-engineered from links produced by the Console.
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

use crate::source_command::{SourceTime, prepend_source_command};

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

#[derive(Debug, Error)]
pub enum ConsoleLinkError {
    #[error("{0}")]
    Decode(String),
    #[error("{0}")]
    Parse(String),
}

/// Inverse of [`encode_rison`].
pub fn decode_rison(input: &str) -> Result<RisonValue, ConsoleLinkError> {
    let mut parser = RisonParser {
        src: input.as_bytes(),
        pos: 0,
    };
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
        let c = self
            .peek()
            .ok_or_else(|| ConsoleLinkError::Decode("unexpected end of input".into()))?;
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
pub struct LogAnalyticsLinkInput<'a> {
    pub region: &'a str,
    /// Log group names (not ARNs) — the `SOURCE` command takes plain names.
    pub log_groups: &'a [String],
    pub time: TimeSpec,
    /// The query pipeline, without a `SOURCE` command; one is prepended.
    pub query: &'a str,
    /// The editor tab's display name. The console leaves the tab unnamed when
    /// this is absent.
    pub label: Option<&'a str>,
    /// The tab's id. Fresh ids keep a link from colliding with a tab the
    /// recipient already has open; [`new_tab_id`] generates one.
    pub tab_id: &'a str,
}

/// Build a shareable URL for the revamped console ("Log Analytics").
pub fn build_log_analytics_link(input: &LogAnalyticsLinkInput<'_>) -> String {
    let (start, end) = match input.time {
        TimeSpec::Relative { seconds_back } => (
            SourceTime::Relative {
                offset_ms: -seconds_back * 1000,
            },
            SourceTime::Relative { offset_ms: 0 },
        ),
        TimeSpec::Absolute { start_ms, end_ms } => (
            SourceTime::Absolute { epoch_ms: start_ms },
            SourceTime::Absolute { epoch_ms: end_ms },
        ),
    };
    let query = prepend_source_command(input.log_groups, Some(start), Some(end), input.query);

    // Parameter order follows the links the console itself produces.
    let mut params = vec![
        ("active".to_string(), "a".to_string()),
        ("a.id".to_string(), input.tab_id.to_string()),
        ("a.pos".to_string(), "0".to_string()),
    ];
    if let Some(label) = input.label {
        params.push(("a.label".to_string(), label.to_string()));
    }
    params.push(("a.type".to_string(), "query".to_string()));
    params.push(("a.query".to_string(), query));

    let encoded: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", encode_param_value(v)))
        .collect();
    format!(
        "https://{region}.console.aws.amazon.com/cloudwatch/home?region={region}\
         #{LOG_ANALYTICS_ROUTE}?{params}",
        region = input.region,
        params = encoded.join("&"),
    )
}

/// Encode one `#log-analytics` parameter value. Each is a Rison string, and the
/// console percent-encodes the `~'` marker that introduces it.
fn encode_param_value(value: &str) -> String {
    format!("%7E%27{}", encode_aws_string(value))
}

/// A fresh tab id, in the UUID form the console uses.
pub fn new_tab_id() -> String {
    uuid::Uuid::new_v4().to_string()
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
    pub detail: LinkDetail,
}

#[derive(Debug)]
pub enum LinkDetail {
    /// Classic Logs Insights links: `#logsV2:logs-insights$3FqueryDetail$3D~(...)`.
    QueryDetail(IndexMap<String, RisonValue>),
    /// Links from the revamped console ("Log Analytics"): `#log-analytics?...`.
    LogAnalytics(LogAnalyticsTab),
}

/// One editor tab of a `#log-analytics` link. The console numbers its tabs with
/// a letter prefix (`a.query`, `b.query`, …) and names the selected one in the
/// `active` parameter; this is the selected tab's fields.
#[derive(Debug, Default, PartialEq)]
pub struct LogAnalyticsTab {
    /// The tab's letter, e.g. `a`.
    pub key: String,
    /// `a.label`, the tab's display name.
    pub label: Option<String>,
    /// `a.type`, e.g. `query`.
    pub kind: Option<String>,
    /// `a.query`, the query text — including its leading `SOURCE ...` command,
    /// which is where this format keeps the log groups and the time window.
    pub query: String,
}

/// Inverse of [`build_console_link`]. Accepts both console URL formats, and
/// fragments using either AWS's `$3F`/`$3D` encoding or standard `%3F`/`%3D`.
pub fn parse_console_link(url: &str) -> Result<ParsedConsoleLink, ConsoleLinkError> {
    let trimmed = url.trim();
    let hash_idx = trimmed
        .find('#')
        .ok_or_else(|| ConsoleLinkError::Parse("URL has no fragment (no '#')".into()))?;
    let query_str = &trimmed[..hash_idx];
    let fragment = &trimmed[hash_idx + 1..];

    let region = extract_region(query_str).ok_or_else(|| {
        ConsoleLinkError::Parse("URL is missing the ?region=... query parameter".into())
    })?;

    let mut normalized = percent_decode_lossy(fragment);
    normalized = normalized.replace("$3F", "?").replace("$3D", "=");

    // The fragment is its own router: `<route>?<params>`.
    let (route, params) = match normalized.split_once('?') {
        Some((route, params)) => (route, params),
        None => (normalized.as_str(), ""),
    };

    let detail = if route == LOG_ANALYTICS_ROUTE {
        LinkDetail::LogAnalytics(parse_log_analytics(params)?)
    } else {
        LinkDetail::QueryDetail(parse_query_detail(&normalized)?)
    };
    Ok(ParsedConsoleLink { region, detail })
}

const LOG_ANALYTICS_ROUTE: &str = "log-analytics";

fn parse_query_detail(normalized: &str) -> Result<IndexMap<String, RisonValue>, ConsoleLinkError> {
    let detail_str = extract_query_detail(normalized)
        .ok_or_else(|| ConsoleLinkError::Parse("fragment is missing queryDetail=...".into()))?;
    let detail = decode_rison(&detail_str)?;
    let RisonValue::Object(map) = detail else {
        return Err(ConsoleLinkError::Parse(
            "queryDetail is not a Rison object".into(),
        ));
    };
    Ok(map)
}

/// Parse the parameters of a `#log-analytics?...` fragment. Every value is a
/// Rison value preceded by `~`, e.g. `a.label=~'Installer*20App*20BFF`.
fn parse_log_analytics(params: &str) -> Result<LogAnalyticsTab, ConsoleLinkError> {
    let mut values: IndexMap<String, RisonValue> = IndexMap::new();
    for pair in params.split('&').filter(|p| !p.is_empty()) {
        let Some((key, raw)) = pair.split_once('=') else {
            continue;
        };
        let raw = raw.strip_prefix('~').unwrap_or(raw);
        values.insert(key.to_string(), decode_rison(raw)?);
    }

    let key = active_tab_key(&values).ok_or_else(|| {
        ConsoleLinkError::Parse(
            "log-analytics URL has no query tab (no `active=` and no `<tab>.query=` parameter)"
                .into(),
        )
    })?;
    let field = |name: &str| {
        values
            .get(&format!("{key}.{name}"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let query = field("query").ok_or_else(|| {
        ConsoleLinkError::Parse(format!(
            "log-analytics URL has no {key}.query parameter — is this a query tab?"
        ))
    })?;

    Ok(LogAnalyticsTab {
        label: field("label"),
        kind: field("type"),
        query,
        key,
    })
}

/// The tab named by `active`, or — when that is missing or names a tab with no
/// parameters of its own — the first tab that carries a query.
fn active_tab_key(values: &IndexMap<String, RisonValue>) -> Option<String> {
    let tab_keys = || {
        values
            .keys()
            .filter_map(|k| k.split_once('.'))
            .map(|(prefix, _)| prefix)
    };
    if let Some(active) = values.get("active").and_then(|v| v.as_str()) {
        if tab_keys().any(|k| k == active) {
            return Some(active.to_string());
        }
    }
    values
        .keys()
        .filter_map(|k| k.strip_suffix(".query"))
        .next()
        .map(|k| k.to_string())
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
            if let Ok(b) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
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
    fn decode_rison_values() {
        assert_eq!(decode_rison("0").unwrap(), n(0.0));
        assert_eq!(decode_rison("-3600").unwrap(), n(-3600.0));
        assert_eq!(decode_rison("'UTC").unwrap(), s("UTC"));
        assert_eq!(
            decode_rison("'fields*20*40timestamp*0a*7c*20limit*20200").unwrap(),
            s("fields @timestamp\n| limit 200")
        );
        assert_eq!(
            decode_rison("(~'a*3ab~'c*2fd)").unwrap(),
            RisonValue::Array(vec![s("a:b"), s("c/d")])
        );
    }

    #[test]
    fn decode_rison_object_preserves_key_order() {
        let RisonValue::Object(obj) = decode_rison("(z~1~a~2~m~3)").unwrap() else {
            panic!("expected an object");
        };
        let keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        assert_eq!(keys, vec!["z", "a", "m"]);
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

    fn expect_query_detail(parsed: &ParsedConsoleLink) -> &IndexMap<String, RisonValue> {
        match &parsed.detail {
            LinkDetail::QueryDetail(map) => map,
            other => panic!("expected a queryDetail link, got {other:?}"),
        }
    }

    fn expect_log_analytics(parsed: &ParsedConsoleLink) -> &LogAnalyticsTab {
        match &parsed.detail {
            LinkDetail::LogAnalytics(tab) => tab,
            other => panic!("expected a log-analytics link, got {other:?}"),
        }
    }

    /// A classic Logs Insights link, as the console produced them before the
    /// "Log Analytics" revamp. We no longer build these, only read them.
    const CLASSIC_URL: &str = concat!(
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

    #[test]
    fn parse_classic_link() {
        let parsed = parse_console_link(CLASSIC_URL).unwrap();
        assert_eq!(parsed.region, "eu-north-1");
        let detail = expect_query_detail(&parsed);
        let keys: Vec<&str> = detail.keys().map(|k| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "end",
                "start",
                "timeType",
                "tz",
                "unit",
                "editorString",
                "queryId",
                "source",
                "lang",
                "logClass",
                "queryBy"
            ]
        );
        assert_eq!(detail.get("start").unwrap().as_f64(), Some(-3600.0));
        assert_eq!(detail.get("timeType").unwrap().as_str(), Some("RELATIVE"));
        assert_eq!(
            detail.get("editorString").unwrap().as_str(),
            Some(
                "fields @timestamp, @message\n| sort @timestamp desc\n\
                 | filter app = 'my-service'\n| limit 200"
            )
        );
        assert_eq!(
            detail.get("source").unwrap().as_array().unwrap()[0].as_str(),
            Some("arn:aws:logs:eu-north-1:123456789012:log-group:/my/service/logs")
        );
    }

    #[test]
    fn parse_classic_link_tolerates_percent_encoding() {
        let percent = CLASSIC_URL
            .replacen("$3F", "%3F", 1)
            .replacen("$3D", "%3D", 1);
        let parsed = parse_console_link(&percent).unwrap();
        assert_eq!(parsed.region, "eu-north-1");
        assert_eq!(
            *expect_query_detail(&parsed),
            *expect_query_detail(&parse_console_link(CLASSIC_URL).unwrap())
        );
    }

    #[test]
    fn parse_console_link_rejects_missing_query_detail() {
        let url = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1#logsV2:logs-insights";
        let err = parse_console_link(url).unwrap_err();
        assert!(format!("{err}").contains("queryDetail"));
    }

    /// A real link copied out of the revamped ("Log Analytics") console.
    const LOG_ANALYTICS_URL: &str = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home\
        ?region=eu-north-1#log-analytics?active=%7E%27a\
        &a.id=%7E%27314f1fbc-9bac-41c7-90d1-f8b54d6b073f&a.pos=%7E%270\
        &a.label=%7E%27Installer*20App*20BFF&a.type=%7E%27query\
        &a.query=%7E%27SOURCE*20*22*2feks*2fprod*2fteam-icc*22*20START*3d-1w*20END*3d0s*20*7c\
        *0afields*20*40timestamp*2c*20level*2c*20methodName*2c*20message\
        *0a*7c*20sort*20*40timestamp*20desc\
        *0a*7c*20filter*20ispresent*28message*29*20and*20app*20*3d*20*27acquisition-installer-app-bff*27\
        *0a*7c*20filter*20userId*20*3d*20*2795c18c39-352d-4e95-8c0c-05a016e3e8d4*27\
        *0a*7c*20limit*20200\
        &a.sqedit=%7E%27fbb8b045-7290-4f56-ba2e-83cbafe4bfb2\
        &a.sqname=%7E%27team-icc*2fInstaller*20App*20BFF";

    #[test]
    fn parse_log_analytics_link() {
        let parsed = parse_console_link(LOG_ANALYTICS_URL).unwrap();
        assert_eq!(parsed.region, "eu-north-1");
        let tab = expect_log_analytics(&parsed);
        assert_eq!(tab.key, "a");
        assert_eq!(tab.label.as_deref(), Some("Installer App BFF"));
        assert_eq!(tab.kind.as_deref(), Some("query"));
        assert_eq!(
            tab.query,
            "SOURCE \"/eks/prod/team-icc\" START=-1w END=0s |\n\
             fields @timestamp, level, methodName, message\n\
             | sort @timestamp desc\n\
             | filter ispresent(message) and app = 'acquisition-installer-app-bff'\n\
             | filter userId = '95c18c39-352d-4e95-8c0c-05a016e3e8d4'\n\
             | limit 200"
        );
    }

    #[test]
    fn parse_log_analytics_picks_the_active_tab() {
        let url = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1\
            #log-analytics?a.query=%7E%27SOURCE*20*22*2fa*22&active=%7E%27b\
            &b.query=%7E%27SOURCE*20*22*2fb*22&b.type=%7E%27query";
        let parsed = parse_console_link(url).unwrap();
        let tab = expect_log_analytics(&parsed);
        assert_eq!(tab.key, "b");
        assert_eq!(tab.query, "SOURCE \"/b\"");
    }

    #[test]
    fn parse_log_analytics_falls_back_to_first_query_tab() {
        // `active` naming a tab that carries no parameters of its own.
        let url = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1\
            #log-analytics?active=%7E%27z&a.query=%7E%27SOURCE*20*22*2fa*22";
        let parsed = parse_console_link(url).unwrap();
        assert_eq!(expect_log_analytics(&parsed).key, "a");
    }

    #[test]
    fn parse_log_analytics_rejects_tab_without_a_query() {
        let url = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1\
            #log-analytics?active=%7E%27a&a.label=%7E%27Empty";
        let err = parse_console_link(url).unwrap_err();
        assert!(format!("{err}").contains("a.query"));
    }

    #[test]
    fn build_log_analytics_link_byte_for_byte() {
        let expected = concat!(
            "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1",
            "#log-analytics?active=%7E%27a",
            "&a.id=%7E%27dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
            "&a.pos=%7E%270",
            "&a.label=%7E%27my-service",
            "&a.type=%7E%27query",
            "&a.query=%7E%27SOURCE*20*22*2fmy*2fservice*2flogs*22*20START*3d-1h*20END*3d0s*20*7c",
            "*0afields*20*40timestamp*2c*20*40message",
            "*0a*7c*20sort*20*40timestamp*20desc",
            "*0a*7c*20filter*20app*20*3d*20*27my-service*27",
            "*0a*7c*20limit*20200",
        );
        let log_groups = vec!["/my/service/logs".to_string()];
        assert_eq!(
            build_log_analytics_link(&LogAnalyticsLinkInput {
                region: "eu-north-1",
                log_groups: &log_groups,
                time: TimeSpec::Relative { seconds_back: 3600 },
                query: "fields @timestamp, @message\n| sort @timestamp desc\n\
                        | filter app = 'my-service'\n| limit 200",
                label: Some("my-service"),
                tab_id: "dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
            }),
            expected
        );
    }

    #[test]
    fn build_log_analytics_link_omits_an_absent_label() {
        let log_groups = vec!["/x".to_string()];
        let url = build_log_analytics_link(&LogAnalyticsLinkInput {
            region: "eu-north-1",
            log_groups: &log_groups,
            time: TimeSpec::Relative { seconds_back: 60 },
            query: "fields @timestamp",
            label: None,
            tab_id: "id",
        });
        assert!(!url.contains("a.label"), "{url}");
    }

    #[test]
    fn build_log_analytics_link_absolute_window() {
        let log_groups = vec!["/x".to_string()];
        let url = build_log_analytics_link(&LogAnalyticsLinkInput {
            region: "eu-north-1",
            log_groups: &log_groups,
            time: TimeSpec::Absolute {
                start_ms: 1_700_000_000_000,
                end_ms: 1_700_003_600_000,
            },
            query: "fields @timestamp",
            label: None,
            tab_id: "id",
        });
        let parsed = parse_console_link(&url).unwrap();
        assert_eq!(
            expect_log_analytics(&parsed).query,
            "SOURCE \"/x\" START=1700000000000 END=1700003600000 |\nfields @timestamp"
        );
    }

    #[test]
    fn build_log_analytics_link_round_trips_through_the_parser() {
        let log_groups = vec!["/a".to_string(), "/b with space".to_string()];
        let query = "fields @timestamp, @message\n| filter app = 'x'\n| limit 20";
        let url = build_log_analytics_link(&LogAnalyticsLinkInput {
            region: "eu-north-1",
            log_groups: &log_groups,
            time: TimeSpec::Relative {
                seconds_back: 604_800,
            },
            query,
            label: Some("A & B"),
            tab_id: "dae7095d-9b56-4ec6-ab9a-d2ffb41b0fdb",
        });
        let parsed = parse_console_link(&url).unwrap();
        assert_eq!(parsed.region, "eu-north-1");
        let tab = expect_log_analytics(&parsed);
        assert_eq!(tab.key, "a");
        assert_eq!(tab.label.as_deref(), Some("A & B"));
        assert_eq!(tab.kind.as_deref(), Some("query"));
        assert_eq!(
            tab.query,
            format!("SOURCE \"/a\", \"/b with space\" START=-1w END=0s |\n{query}")
        );
    }

    #[test]
    fn new_tab_id_is_a_fresh_uuid() {
        let a = new_tab_id();
        assert_ne!(a, new_tab_id());
        assert_eq!(a.len(), 36);
    }
}
