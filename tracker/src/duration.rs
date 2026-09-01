//! A single format for durations, used both when reading and writing them.
//!
//! Durations are written in one of two forms:
//!
//! * **Compound** – a single term, optionally signed: `1h`, `+1h`, `1:26h`
//!   (one hour and 26 minutes), `-1:26h`, `45m`, `-45m`. The part after the
//!   colon is minutes, and must be exactly two digits, so that `1:30h` can
//!   never be confused with a decimal fraction of an hour.
//! * **Separated** – several terms, each with its own sign, added together:
//!   `1h 30m`, `+1h -30m` (equivalent to `+30m`), `-2h -15m`.
//!
//! Output always uses the compound form: whole hours print as `8h`, durations
//! shorter than an hour print as `30m`, everything else as `1:26h`, and zero
//! prints as `0h`.

use chrono::{Duration, TimeDelta};
use regex::Regex;
use std::fmt;
use std::sync::LazyLock;

static TERM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?P<sign>[-+])?\s*(?:(?P<hours>[0-9]+)(?::(?P<minutes>[0-9]{2}))?\s*h|(?P<only_minutes>[0-9]+)\s*m)",
    )
    .unwrap()
});

/// Catches `1:5h`, so it can be reported as the mistake it is rather than as
/// an unexpected `1:`. There is no lookahead in this regex crate, hence the
/// explicit "not followed by another digit".
static SINGLE_DIGIT_MINUTES_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9]:[0-9]([^0-9]|$)").unwrap());

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ParseDurationError {
    /// Nothing that could be read as a duration term.
    Empty(String),
    /// The minutes after the colon are written with a single digit.
    SingleDigitMinutes,
    /// There is something in the input that isn't part of a duration term.
    Unexpected(String),
    /// The minutes after the colon are not in the range 00–59.
    MinutesOutOfRange(u32),
    /// The duration is too large to represent.
    OutOfRange(String),
}

impl fmt::Display for ParseDurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseDurationError::Empty(input) => {
                write!(
                    f,
                    "`{}` is not a duration, expected something like 1:30h",
                    input
                )
            }
            ParseDurationError::SingleDigitMinutes => {
                write!(
                    f,
                    "minutes after the colon must be written with two digits, like 1:05h"
                )
            }
            ParseDurationError::Unexpected(rest) => {
                write!(f, "unexpected `{}` in duration", rest.trim())
            }
            ParseDurationError::MinutesOutOfRange(minutes) => {
                write!(
                    f,
                    "minutes after the colon must be 00–59, got {:02}",
                    minutes
                )
            }
            ParseDurationError::OutOfRange(input) => {
                write!(f, "duration `{}` is too large", input)
            }
        }
    }
}

/// Parses a duration in either the compound or the separated form.
pub fn parse_duration(string: &str) -> Result<Duration, ParseDurationError> {
    if SINGLE_DIGIT_MINUTES_REGEX.is_match(string) {
        return Err(ParseDurationError::SingleDigitMinutes);
    }
    let mut minutes: i64 = 0;
    let mut terms = 0;
    let mut position = 0;

    for captures in TERM_REGEX.captures_iter(string) {
        let whole = captures.get(0).unwrap();
        // Terms may only be separated by whitespace.
        let gap = &string[position..whole.start()];
        if !gap.trim().is_empty() {
            return Err(ParseDurationError::Unexpected(gap.to_string()));
        }
        position = whole.end();
        terms += 1;

        let term_minutes = match captures.name("only_minutes") {
            Some(only_minutes) => parse_number(only_minutes.as_str(), string)?,
            None => {
                let hours = parse_number(captures.name("hours").unwrap().as_str(), string)?;
                let minutes_after_colon = match captures.name("minutes") {
                    Some(minutes) => {
                        let minutes = parse_number(minutes.as_str(), string)?;
                        if minutes > 59 {
                            return Err(ParseDurationError::MinutesOutOfRange(minutes as u32));
                        }
                        minutes
                    }
                    None => 0,
                };
                hours
                    .checked_mul(60)
                    .and_then(|hours| hours.checked_add(minutes_after_colon))
                    .ok_or_else(|| ParseDurationError::OutOfRange(string.to_string()))?
            }
        };

        let signed = match captures.name("sign").map(|sign| sign.as_str()) {
            Some("-") => -term_minutes,
            _ => term_minutes,
        };
        minutes = minutes
            .checked_add(signed)
            .ok_or_else(|| ParseDurationError::OutOfRange(string.to_string()))?;
    }

    if terms == 0 {
        return Err(ParseDurationError::Empty(string.to_string()));
    }
    let rest = &string[position..];
    if !rest.trim().is_empty() {
        return Err(ParseDurationError::Unexpected(rest.to_string()));
    }

    TimeDelta::try_minutes(minutes)
        .ok_or_else(|| ParseDurationError::OutOfRange(string.to_string()))
}

fn parse_number(string: &str, input: &str) -> Result<i64, ParseDurationError> {
    string
        .parse::<i64>()
        .map_err(|_| ParseDurationError::OutOfRange(input.to_string()))
}

/// Formats a duration in the compound form, with a sign only when negative.
pub fn format_duration(duration: Duration) -> String {
    format(duration, false)
}

/// Formats a duration in the compound form, with an explicit `+` when positive.
/// Used where the duration is a balance, and its direction is the point.
pub fn format_signed_duration(duration: Duration) -> String {
    format(duration, true)
}

fn format(duration: Duration, explicit_plus: bool) -> String {
    let total_minutes = duration.num_minutes();
    if total_minutes == 0 {
        return String::from("0h");
    }
    let sign = if total_minutes < 0 {
        "-"
    } else if explicit_plus {
        "+"
    } else {
        ""
    };
    let hours = total_minutes.abs() / 60;
    let minutes = total_minutes.abs() % 60;
    match (hours, minutes) {
        (0, minutes) => format!("{}{}m", sign, minutes),
        (hours, 0) => format!("{}{}h", sign, hours),
        (hours, minutes) => format!("{}{}:{:02}h", sign, hours, minutes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minutes(minutes: i64) -> Duration {
        TimeDelta::try_minutes(minutes).unwrap()
    }

    #[test]
    fn parses_compound_form() {
        assert_eq!(Ok(minutes(60)), parse_duration("1h"));
        assert_eq!(Ok(minutes(60)), parse_duration("+1h"));
        assert_eq!(Ok(minutes(-60)), parse_duration("-1h"));
        assert_eq!(Ok(minutes(86)), parse_duration("1:26h"));
        assert_eq!(Ok(minutes(86)), parse_duration("+1:26h"));
        assert_eq!(Ok(minutes(-86)), parse_duration("-1:26h"));
        assert_eq!(Ok(minutes(65)), parse_duration("1:05h"));
        assert_eq!(Ok(minutes(30)), parse_duration("30m"));
        assert_eq!(Ok(minutes(-30)), parse_duration("-30m"));
        assert_eq!(Ok(minutes(0)), parse_duration("0h"));
    }

    #[test]
    fn parses_separated_form() {
        assert_eq!(Ok(minutes(90)), parse_duration("1h 30m"));
        assert_eq!(Ok(minutes(30)), parse_duration("+1h -30m"));
        assert_eq!(Ok(minutes(-135)), parse_duration("-2h -15m"));
        assert_eq!(Ok(minutes(-105)), parse_duration("-2h +15m"));
    }

    #[test]
    fn parses_legacy_spacing() {
        // Week files written by earlier versions look like this.
        assert_eq!(Ok(minutes(1200)), parse_duration("20 h 0 m"));
        assert_eq!(Ok(minutes(-306)), parse_duration("-5h -6m"));
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        assert_eq!(Ok(minutes(90)), parse_duration("  1:30h  "));
    }

    #[test]
    fn rejects_single_digit_minutes() {
        // `1:5h` is too easy to read as an hour and a half.
        assert_eq!(
            Err(ParseDurationError::SingleDigitMinutes),
            parse_duration("1:5h")
        );
        assert_eq!(
            Err(ParseDurationError::SingleDigitMinutes),
            parse_duration("10:5h")
        );
        assert_eq!(Ok(minutes(605)), parse_duration("10:05h"));
    }

    #[test]
    fn rejects_minutes_above_59() {
        assert_eq!(
            Err(ParseDurationError::MinutesOutOfRange(60)),
            parse_duration("1:60h")
        );
    }

    #[test]
    fn rejects_non_durations() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("vacation").is_err());
        assert!(parse_duration("1.30h").is_err());
        assert!(parse_duration("1h30").is_err());
        assert!(parse_duration("1h potato").is_err());
        assert!(parse_duration("potato 1h").is_err());
    }

    #[test]
    fn formats_compound_form() {
        assert_eq!("0h", format_duration(minutes(0)));
        assert_eq!("8h", format_duration(minutes(8 * 60)));
        assert_eq!("-8h", format_duration(minutes(-8 * 60)));
        assert_eq!("6:30h", format_duration(minutes(390)));
        assert_eq!("-1:30h", format_duration(minutes(-90)));
        assert_eq!("30m", format_duration(minutes(30)));
        assert_eq!("-30m", format_duration(minutes(-30)));
        assert_eq!("1:05h", format_duration(minutes(65)));
    }

    #[test]
    fn formats_signed_form() {
        assert_eq!("0h", format_signed_duration(minutes(0)));
        assert_eq!("+30m", format_signed_duration(minutes(30)));
        assert_eq!("-30m", format_signed_duration(minutes(-30)));
        assert_eq!("+6:30h", format_signed_duration(minutes(390)));
        assert_eq!("-1:30h", format_signed_duration(minutes(-90)));
    }

    #[test]
    fn ignores_seconds() {
        assert_eq!("1m", format_duration(Duration::seconds(119)));
        assert_eq!("-1m", format_duration(Duration::seconds(-119)));
    }

    #[test]
    fn formatted_durations_parse_back() {
        for total in [-1000i64, -90, -59, -1, 0, 1, 59, 90, 1000] {
            let formatted = format_duration(minutes(total));
            assert_eq!(
                Ok(minutes(total)),
                parse_duration(&formatted),
                "{}",
                formatted
            );
            let formatted = format_signed_duration(minutes(total));
            assert_eq!(
                Ok(minutes(total)),
                parse_duration(&formatted),
                "{}",
                formatted
            );
        }
    }
}
