//! Flexible time-range parser.
//!
//! Supported forms (evaluated in order):
//!
//!   1. Relative duration from now: "5h", "30m", "45s", "500ms", "2d", "1w".
//!   2. Day keyword + time range on that day: "yesterday 17-18",
//!      "today 13.00-13.01.30".
//!   3. Bare day keyword: "today", "yesterday".
//!   4. Time-only range on today: "13.00-13.01", "09:15:00.000-09:15:00.500".
//!      Wraps past midnight if `end <= start`.
//!   5. Explicit range with separator "/", "..", or " to ". Either side may
//!      be a bare time (today), an ISO date, or an ISO datetime.

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct TimeRangeParseError(pub String);

const DAY_KEYWORDS: &[&str] = &["today", "yesterday"];

/// Parse a time-range expression. `now` is used as the reference instant.
pub fn parse_time_range(
    input: &str,
    now: DateTime<Local>,
) -> Result<TimeRange, TimeRangeParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TimeRangeParseError("empty time range".into()));
    }

    if let Some(r) = try_parse_duration(trimmed, now) {
        return Ok(r);
    }
    if let Some(r) = try_parse_day_keyword_range(trimmed, now)? {
        return Ok(r);
    }
    if let Some(r) = try_parse_time_only_range(trimmed, now)? {
        return Ok(r);
    }
    if let Some(r) = try_parse_explicit_range(trimmed, now)? {
        return Ok(r);
    }

    Err(TimeRangeParseError(format!(
        "could not parse time range: {input:?}. \
         Supported forms: relative duration (e.g. 1h, 3h, 1d, 30m, 500ms, 1w), \
         day keyword (today, yesterday), day keyword + time range \
         (e.g. \"yesterday 17-18\"), time-only range (e.g. 13:00-14:00), \
         or explicit ISO range (e.g. 2026-04-22T13:00:00Z/2026-04-22T14:00:00Z)"
    )))
}

/// If `input` is a relative-duration shorthand that is a whole number of seconds,
/// return that. Used by `link` to emit RELATIVE time form.
pub fn try_parse_relative_duration_seconds(input: &str) -> Option<i64> {
    let ms = try_parse_duration_ms(input)?;
    if ms % 1000 != 0 {
        return None;
    }
    Some(ms / 1000)
}

/// If `input` is a relative-duration shorthand ("1w", "500ms"), return it in
/// milliseconds.
pub fn try_parse_duration_ms(input: &str) -> Option<i64> {
    let (amount, unit) = split_duration(input.trim())?;
    amount_ms(amount, unit)
}

/// Format a non-negative number of milliseconds as the largest unit that
/// divides it exactly — the inverse of [`try_parse_duration_ms`], and always
/// something that parser accepts back.
pub fn format_duration_ms(total_ms: i64) -> String {
    if total_ms == 0 {
        return "0s".into();
    }
    for (suffix, unit_ms) in [
        ("w", 7 * 86_400_000),
        ("d", 86_400_000),
        ("h", 3_600_000),
        ("m", 60_000),
        ("s", 1_000),
    ] {
        if total_ms % unit_ms == 0 {
            return format!("{}{suffix}", total_ms / unit_ms);
        }
    }
    format!("{total_ms}ms")
}

fn try_parse_duration(input: &str, now: DateTime<Local>) -> Option<TimeRange> {
    let (amount, unit) = split_duration(input)?;
    let ms = amount_ms(amount, unit)?;
    let now_utc = now.with_timezone(&Utc);
    let start = now_utc - Duration::milliseconds(ms);
    Some(TimeRange {
        start,
        end: now_utc,
    })
}

/// Split a string like "5h" into (5.0, "h"). Accepts decimal amounts.
fn split_duration(input: &str) -> Option<(f64, &str)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i == 0 || i == bytes.len() {
        return None;
    }
    let amount_str = &input[..i];
    let unit_str = &input[i..];
    if !matches!(unit_str, "ms" | "s" | "m" | "h" | "d" | "w") {
        return None;
    }
    let amount: f64 = amount_str.parse().ok()?;
    Some((amount, unit_str))
}

fn amount_ms(amount: f64, unit: &str) -> Option<i64> {
    let unit_ms: f64 = match unit {
        "ms" => 1.0,
        "s" => 1_000.0,
        "m" => 60_000.0,
        "h" => 3_600_000.0,
        "d" => 86_400_000.0,
        "w" => 7.0 * 86_400_000.0,
        _ => return None,
    };
    let total = amount * unit_ms;
    if !total.is_finite() {
        return None;
    }
    Some(total as i64)
}

fn try_parse_day_keyword_range(
    input: &str,
    now: DateTime<Local>,
) -> Result<Option<TimeRange>, TimeRangeParseError> {
    if let Some(idx) = input.find(' ') {
        let word = input[..idx].to_ascii_lowercase();
        if !DAY_KEYWORDS.contains(&word.as_str()) {
            return Ok(None);
        }
        let rest = input[idx + 1..].trim();
        let base = resolve_day_keyword(&word, now);
        if let Some(r) = parse_time_range_string(rest, base) {
            return Ok(Some(r));
        }
        return Ok(None);
    }

    let word = input.to_ascii_lowercase();
    if !DAY_KEYWORDS.contains(&word.as_str()) {
        return Ok(None);
    }
    let base = resolve_day_keyword(&word, now);
    Ok(Some(TimeRange {
        start: base,
        end: base + Duration::days(1),
    }))
}

fn try_parse_time_only_range(
    input: &str,
    now: DateTime<Local>,
) -> Result<Option<TimeRange>, TimeRangeParseError> {
    let base = start_of_day(now);
    Ok(parse_time_range_string(input, base))
}

/// Parse "TIME-TIME" where each TIME uses dots/colons (no dashes of its own).
fn parse_time_range_string(input: &str, base: DateTime<Utc>) -> Option<TimeRange> {
    let dash_idx = input.find('-')?;
    if dash_idx == 0 {
        return None;
    }
    let left_raw = input[..dash_idx].trim();
    let right_raw = input[dash_idx + 1..].trim();
    let left = parse_time_of_day_ms(left_raw)?;
    let right = parse_time_of_day_ms(right_raw)?;
    let start = base + Duration::milliseconds(left);
    let mut end = base + Duration::milliseconds(right);
    if end <= start {
        end += Duration::days(1);
    }
    Some(TimeRange { start, end })
}

/// Parse "HH", "HH:MM", "HH.MM.SS", "HH.MM.SS.mmm" etc. Returns ms-of-day.
fn parse_time_of_day_ms(input: &str) -> Option<i64> {
    if input.is_empty() {
        return None;
    }
    // Split on `.` or `:`, but treat the last `.` before a 1-3 digit tail as
    // milliseconds when there are at least 4 segments.
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let bytes = input.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'.' || *b == b':' {
            parts.push(&input[start..i]);
            start = i + 1;
        }
    }
    parts.push(&input[start..]);

    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    for p in &parts {
        if p.is_empty() || p.len() > 3 {
            return None;
        }
        if !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    let h: i64 = parts[0].parse().ok()?;
    let m: i64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let s: i64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let ms_str = parts.get(3).copied().unwrap_or("0");
    // Pad/truncate to 3 digits for milliseconds.
    let ms_padded = format!("{:0<3}", ms_str);
    let ms_padded = &ms_padded[..3];
    let ms: i64 = ms_padded.parse().ok()?;

    if h > 23 || m > 59 || s > 59 {
        return None;
    }
    Some(h * 3_600_000 + m * 60_000 + s * 1_000 + ms)
}

fn try_parse_explicit_range(
    input: &str,
    now: DateTime<Local>,
) -> Result<Option<TimeRange>, TimeRangeParseError> {
    for sep in [" to ", "..", "/"] {
        if let Some(idx) = input.find(sep) {
            let left = input[..idx].trim();
            let right = input[idx + sep.len()..].trim();
            let start = parse_instant(left, now)?;
            let end = parse_instant(right, now)?;
            return Ok(Some(TimeRange { start, end }));
        }
    }
    Ok(None)
}

/// Parse a single instant: bare time-of-day (today), day keyword, ISO date,
/// or ISO datetime. Returns UTC.
fn parse_instant(input: &str, now: DateTime<Local>) -> Result<DateTime<Utc>, TimeRangeParseError> {
    if let Some(ms) = parse_time_of_day_ms(input) {
        return Ok(start_of_day(now) + Duration::milliseconds(ms));
    }

    let lower = input.to_ascii_lowercase();
    if DAY_KEYWORDS.contains(&lower.as_str()) {
        return Ok(resolve_day_keyword(&lower, now));
    }

    // "YYYY-MM-DD HH:MM[:SS[.mmm]]" → swap space for T.
    let normalized =
        if input.len() >= 11 && &input[4..5] == "-" && &input[7..8] == "-" && &input[10..11] == " "
        {
            let mut s = input.to_string();
            s.replace_range(10..11, "T");
            s
        } else {
            input.to_string()
        };

    // Try RFC3339 (with timezone).
    if let Ok(dt) = DateTime::parse_from_rfc3339(&normalized) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Try ISO datetime without timezone (interpret as local time).
    let iso_no_tz_formats = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ];
    for fmt in iso_no_tz_formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&normalized, fmt) {
            return Ok(local_to_utc(naive, now));
        }
    }
    // Try ISO date (interpret as local-time start of day).
    if let Ok(date) = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d") {
        let naive = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).expect("00:00:00"));
        return Ok(local_to_utc(naive, now));
    }

    Err(TimeRangeParseError(format!(
        "could not parse instant: {input:?}"
    )))
}

fn local_to_utc(naive: NaiveDateTime, now: DateTime<Local>) -> DateTime<Utc> {
    let tz = now.timezone();
    tz.from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        // Fall back to UTC interpretation if the local time is ambiguous.
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive))
}

fn start_of_day(now: DateTime<Local>) -> DateTime<Utc> {
    let date = now.date_naive();
    let naive = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).expect("00:00:00"));
    local_to_utc(naive, now)
}

fn resolve_day_keyword(word: &str, now: DateTime<Local>) -> DateTime<Utc> {
    let base = start_of_day(now);
    match word {
        "today" => base,
        "yesterday" => base - Duration::days(1),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Local> {
        // 2026-04-22 15:30:00 local
        Local
            .with_ymd_and_hms(2026, 4, 22, 15, 30, 0)
            .single()
            .unwrap()
    }

    fn local_dt(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32) -> DateTime<Utc> {
        let local = Local.with_ymd_and_hms(y, mo, d, h, mi, s).single().unwrap()
            + Duration::milliseconds(ms as i64);
        local.with_timezone(&Utc)
    }

    fn local_day(y: i32, mo: u32, d: u32) -> DateTime<Utc> {
        local_dt(y, mo, d, 0, 0, 0, 0)
    }

    #[test]
    fn relative_duration_hours() {
        let r = parse_time_range("5h", now()).unwrap();
        assert_eq!(r.end, now().with_timezone(&Utc));
        assert_eq!(r.start, r.end - Duration::hours(5));
    }

    #[test]
    fn relative_durations_other_units() {
        for (input, dur) in [
            ("30m", Duration::minutes(30)),
            ("45s", Duration::seconds(45)),
            ("500ms", Duration::milliseconds(500)),
            ("2d", Duration::days(2)),
            ("1w", Duration::weeks(1)),
        ] {
            let r = parse_time_range(input, now()).unwrap();
            assert_eq!(r.end, now().with_timezone(&Utc), "{input} end");
            assert_eq!(r.start, r.end - dur, "{input} start");
        }
    }

    #[test]
    fn time_only_range_dots() {
        let r = parse_time_range("13.00-13.01", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 13, 0, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 13, 1, 0, 0));
    }

    #[test]
    fn time_only_range_colons() {
        let r = parse_time_range("09:00-10:00", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 9, 0, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 10, 0, 0, 0));
    }

    #[test]
    fn time_only_range_seconds() {
        let r = parse_time_range("13.00.30-13.01.15", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 13, 0, 30, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 13, 1, 15, 0));
    }

    #[test]
    fn time_only_range_milliseconds() {
        let r = parse_time_range("09:15:00.000-09:15:00.500", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 9, 15, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 9, 15, 0, 500));
    }

    #[test]
    fn time_only_range_wraps_midnight() {
        let r = parse_time_range("23.30-00.30", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 23, 30, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 23, 0, 30, 0, 0));
    }

    #[test]
    fn day_keyword_yesterday() {
        let r = parse_time_range("yesterday 17-18", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 21, 17, 0, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 21, 18, 0, 0, 0));
    }

    #[test]
    fn day_keyword_today_seconds() {
        let r = parse_time_range("today 13.00-13.01.30", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 13, 0, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 13, 1, 30, 0));
    }

    #[test]
    fn bare_day_keyword() {
        let r = parse_time_range("yesterday", now()).unwrap();
        assert_eq!(r.start, local_day(2026, 4, 21));
        assert_eq!(r.end, local_day(2026, 4, 22));
    }

    #[test]
    fn explicit_iso_range_slash() {
        let r = parse_time_range("2026-04-22T13:00:00Z/2026-04-22T14:00:00Z", now()).unwrap();
        assert_eq!(
            r.start.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-04-22T13:00:00.000Z"
        );
        assert_eq!(
            r.end.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-04-22T14:00:00.000Z"
        );
    }

    #[test]
    fn explicit_range_dotdot_with_space() {
        let r = parse_time_range("2026-04-22 13:00..2026-04-22 14:00", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 13, 0, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 14, 0, 0, 0));
    }

    #[test]
    fn explicit_range_to_with_bare_times() {
        let r = parse_time_range("13.00 to 14.00", now()).unwrap();
        assert_eq!(r.start, local_dt(2026, 4, 22, 13, 0, 0, 0));
        assert_eq!(r.end, local_dt(2026, 4, 22, 14, 0, 0, 0));
    }

    #[test]
    fn empty_input_rejected() {
        assert!(parse_time_range("", now()).is_err());
        assert!(parse_time_range("   ", now()).is_err());
    }

    #[test]
    fn garbage_rejected() {
        assert!(parse_time_range("not a time", now()).is_err());
    }

    #[test]
    fn natural_language_no_longer_supported() {
        // "from X to Y" is no longer parseable — left side has stray "from ".
        assert!(parse_time_range("from 2026-04-22 13:00 to 2026-04-22 14:00", now()).is_err());
    }

    #[test]
    fn try_parse_relative_duration_seconds_basic() {
        assert_eq!(try_parse_relative_duration_seconds("1h"), Some(3600));
        assert_eq!(try_parse_relative_duration_seconds("30m"), Some(1800));
        assert_eq!(try_parse_relative_duration_seconds("45s"), Some(45));
        assert_eq!(try_parse_relative_duration_seconds("2d"), Some(172800));
        assert_eq!(try_parse_relative_duration_seconds(" 5h "), Some(18000));
    }

    #[test]
    fn try_parse_relative_duration_seconds_rejects_subsecond_and_other() {
        assert_eq!(try_parse_relative_duration_seconds("500ms"), None);
        assert_eq!(try_parse_relative_duration_seconds("yesterday 17-18"), None);
        assert_eq!(try_parse_relative_duration_seconds("13.00-13.01"), None);
        assert_eq!(try_parse_relative_duration_seconds(""), None);
    }
}
