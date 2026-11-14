//! Parse the `SOURCE …` command that heads a CloudWatch "Log Analytics" query.
//!
//! The revamped console (the `#log-analytics` route) no longer carries the log
//! groups and the time window beside the query — it prepends them to the query
//! text instead:
//!
//! ```text
//! SOURCE "/eks/prod/team-icc" START=-1w END=0s |
//! fields @timestamp, @message
//! ```
//!
//! Several log groups take one `SOURCE` command each, with the time window on
//! the first. The console does not understand a comma-separated list: it keeps
//! the first name and folds the rest into the query text. Links the console
//! produces itself name log groups by ARN and may start with an empty
//! `SOURCE ""` that carries only the time window:
//!
//! ```text
//! SOURCE "" START=2026-10-21T09:14:30.000Z END=2026-10-21T09:17:00.000Z |
//! SOURCE "arn:aws:logs:eu-north-1:123456789012:log-group:/eks/prod/team-icc" |
//! SOURCE "arn:aws:logs:eu-north-1:123456789012:log-group:/eks/prod/team-pi" |
//! fields @timestamp, @message
//! ```
//!
//! Only the explicit-name form maps back onto log group names. The
//! `logGroups(…)` / `logGroupTags(…)` / `dataSource(…)` selectors from the
//! query-syntax docs describe a *set* of log groups that only CloudWatch can
//! resolve, so those are reported separately and left for the caller to
//! complain about.

use thiserror::Error;

use crate::console_link::parse_log_group_arn;
use crate::time_range::{format_duration_ms, try_parse_duration_ms};

#[derive(Debug, Error)]
#[error("{0}")]
pub struct SourceParseError(pub String);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceTime {
    /// Offset from "now" in milliseconds; negative points into the past.
    Relative { offset_ms: i64 },
    /// Absolute epoch milliseconds.
    Absolute { epoch_ms: i64 },
}

#[derive(Debug, Default, PartialEq)]
pub struct SourceCommand {
    /// Log groups named explicitly, in the order they appear.
    pub log_groups: Vec<String>,
    /// Selector functions we can't resolve locally, e.g. `logGroups`.
    pub unresolved_selectors: Vec<String>,
    pub start: Option<SourceTime>,
    pub end: Option<SourceTime>,
}

/// Split a query that starts with one or more `SOURCE …` commands into those
/// commands, merged, and the rest of the pipeline. Returns `None` when the
/// query has no `SOURCE` prefix.
pub fn split_source_command(
    query: &str,
) -> Result<Option<(SourceCommand, String)>, SourceParseError> {
    let mut rest = query.trim_start();
    if !starts_with_source_keyword(rest) {
        return Ok(None);
    }
    let mut merged = SourceCommand::default();
    while starts_with_source_keyword(rest) {
        let (head, tail) = split_at_top_level_pipe(rest);
        let command = parse_source_command(head["SOURCE".len()..].trim())?;
        merged.log_groups.extend(command.log_groups);
        merged
            .unresolved_selectors
            .extend(command.unresolved_selectors);
        merged.start = merged.start.or(command.start);
        merged.end = merged.end.or(command.end);
        rest = tail.trim_start();
    }
    Ok(Some((merged, rest.to_string())))
}

fn starts_with_source_keyword(s: &str) -> bool {
    let Some(rest) = s.get(..6) else {
        return false;
    };
    if !rest.eq_ignore_ascii_case("SOURCE") {
        return false;
    }
    // `SOURCEFUL` is not a SOURCE command; a keyword must end the word.
    match s.as_bytes().get(6) {
        None => true,
        Some(b) => !(b.is_ascii_alphanumeric() || *b == b'_'),
    }
}

/// Return everything before the first `|` that is outside quotes and brackets,
/// plus everything after it.
fn split_at_top_level_pipe(s: &str) -> (&str, &str) {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                '|' if depth == 0 => return (&s[..i], &s[i + 1..]),
                _ => {}
            },
        }
    }
    (s, "")
}

fn parse_source_command(body: &str) -> Result<SourceCommand, SourceParseError> {
    let mut command = SourceCommand::default();
    for token in tokenize(body)? {
        if let Some(value) = strip_keyword(&token, "START") {
            command.start = Some(parse_source_time(value, "START")?);
        } else if let Some(value) = strip_keyword(&token, "END") {
            command.end = Some(parse_source_time(value, "END")?);
        } else if let Some(name) = selector_function_name(&token) {
            command.unresolved_selectors.push(name);
        } else {
            let name = unquote(&token);
            if name.is_empty() {
                continue;
            }
            match parse_log_group_arn(&name) {
                Some((_, _, name)) => command.log_groups.push(name),
                None => command.log_groups.push(name),
            }
        }
    }
    Ok(command)
}

/// Split on whitespace and commas that are outside quotes and brackets.
fn tokenize(s: &str) -> Result<Vec<String>, SourceParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    current.push(c);
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    current.push(c);
                }
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    current.push(c);
                }
                _ if depth == 0 && (c.is_whitespace() || c == ',') => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(c),
            },
        }
    }
    if quote.is_some() {
        return Err(SourceParseError(format!(
            "unterminated quoted string in SOURCE command: {s:?}"
        )));
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

/// `START=-1w` with keyword `START` yields `Some("-1w")`, case-insensitively.
fn strip_keyword<'a>(token: &'a str, keyword: &str) -> Option<&'a str> {
    let (name, value) = token.split_once('=')?;
    name.trim().eq_ignore_ascii_case(keyword).then_some(value)
}

/// `logGroups(namePrefix: ['x'])` yields `Some("logGroups")`.
fn selector_function_name(token: &str) -> Option<String> {
    let open = token.find('(')?;
    let name = &token[..open];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

fn parse_source_time(raw: &str, keyword: &str) -> Result<SourceTime, SourceParseError> {
    let value = unquote(raw.trim());
    let value = value.trim();
    if value.is_empty() {
        return Err(SourceParseError(format!("{keyword}= has no value")));
    }

    // Relative offsets carry a unit suffix: "-1w", "0s", "-500ms".
    let (sign, magnitude) = match value.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, value.strip_prefix('+').unwrap_or(value)),
    };
    if let Some(ms) = try_parse_duration_ms(magnitude) {
        return Ok(SourceTime::Relative {
            offset_ms: sign * ms,
        });
    }

    // Bare numbers are epoch timestamps — seconds or milliseconds.
    if let Ok(n) = value.parse::<i64>() {
        let epoch_ms = if n.abs() >= 100_000_000_000 {
            n
        } else {
            n * 1000
        };
        return Ok(SourceTime::Absolute { epoch_ms });
    }

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(SourceTime::Absolute {
            epoch_ms: dt.timestamp_millis(),
        });
    }

    Err(SourceParseError(format!(
        "could not parse {keyword}={value:?} in the SOURCE command — expected a \
         relative offset (e.g. -1w, 0s), an epoch timestamp, or an RFC 3339 datetime"
    )))
}

/// Render the `SOURCE …` commands that head a Log Analytics query and prepend
/// them to `query` — one per log group, with the time window on the first. The
/// inverse of [`split_source_command`] for the explicit log-group form that
/// this tool builds.
pub fn prepend_source_command(
    log_groups: &[String],
    start: Option<SourceTime>,
    end: Option<SourceTime>,
    query: &str,
) -> String {
    let mut window = String::new();
    if let Some(start) = start {
        window.push_str(&format!(" START={}", format_source_time(start)));
    }
    if let Some(end) = end {
        window.push_str(&format!(" END={}", format_source_time(end)));
    }
    let mut commands: Vec<String> = log_groups
        .iter()
        .map(|g| format!("SOURCE \"{g}\""))
        .collect();
    if let Some(first) = commands.first_mut() {
        first.push_str(&window);
    }
    // The console puts the pipe on each SOURCE line and the pipeline below.
    let mut rendered = String::new();
    for command in commands {
        rendered.push_str(&command);
        rendered.push_str(" |\n");
    }
    rendered.push_str(query);
    rendered
}

fn format_source_time(time: SourceTime) -> String {
    match time {
        SourceTime::Relative { offset_ms } if offset_ms < 0 => {
            format!("-{}", format_duration_ms(-offset_ms))
        }
        SourceTime::Relative { offset_ms } => format_duration_ms(offset_ms),
        SourceTime::Absolute { epoch_ms } => epoch_ms.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(q: &str) -> (SourceCommand, String) {
        split_source_command(q).unwrap().unwrap()
    }

    #[test]
    fn no_source_prefix_returns_none() {
        assert!(
            split_source_command("fields @timestamp | limit 20")
                .unwrap()
                .is_none()
        );
        assert!(
            split_source_command("sourceIp | stats count()")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn console_form_with_relative_window() {
        let (cmd, rest) = split(
            "SOURCE \"/eks/prod/team-icc\" START=-1w END=0s |\nfields @timestamp\n| limit 200",
        );
        assert_eq!(cmd.log_groups, vec!["/eks/prod/team-icc".to_string()]);
        assert!(cmd.unresolved_selectors.is_empty());
        assert_eq!(
            cmd.start,
            Some(SourceTime::Relative {
                offset_ms: -604_800_000
            })
        );
        assert_eq!(cmd.end, Some(SourceTime::Relative { offset_ms: 0 }));
        assert_eq!(rest, "fields @timestamp\n| limit 200");
    }

    #[test]
    fn one_source_command_per_log_group() {
        let (cmd, rest) = split(
            "SOURCE \"/a\" START=-1h END=0s |\nSOURCE \"/b\" |\nfields @timestamp\n| limit 20",
        );
        assert_eq!(cmd.log_groups, vec!["/a".to_string(), "/b".to_string()]);
        assert_eq!(
            cmd.start,
            Some(SourceTime::Relative {
                offset_ms: -3_600_000
            })
        );
        assert_eq!(cmd.end, Some(SourceTime::Relative { offset_ms: 0 }));
        assert_eq!(rest, "fields @timestamp\n| limit 20");
    }

    #[test]
    fn console_form_with_empty_source_and_arns() {
        let (cmd, rest) = split(
            "SOURCE \"\" START=2026-10-21T09:14:30.000Z END=2026-10-21T09:17:00.000Z | \
             SOURCE \"arn:aws:logs:eu-north-1:123456789012:log-group:/eks/prod/team-icc\" | \
             SOURCE \"arn:aws:logs:eu-north-1:123456789012:log-group:/eks/prod/team-pi\" |\n\
             fields @timestamp",
        );
        assert_eq!(
            cmd.log_groups,
            vec![
                "/eks/prod/team-icc".to_string(),
                "/eks/prod/team-pi".to_string()
            ]
        );
        assert_eq!(
            cmd.start,
            Some(SourceTime::Absolute {
                epoch_ms: 1_792_574_070_000
            })
        );
        assert_eq!(
            cmd.end,
            Some(SourceTime::Absolute {
                epoch_ms: 1_792_574_220_000
            })
        );
        assert_eq!(rest, "fields @timestamp");
    }

    #[test]
    fn several_log_groups_comma_separated() {
        let (cmd, rest) = split("SOURCE \"/a\", \"/b\" | fields @timestamp");
        assert_eq!(cmd.log_groups, vec!["/a".to_string(), "/b".to_string()]);
        assert_eq!(cmd.start, None);
        assert_eq!(cmd.end, None);
        assert_eq!(rest, "fields @timestamp");
    }

    #[test]
    fn bare_and_single_quoted_names() {
        let (cmd, _) = split("SOURCE /a 'b c' | fields @timestamp");
        assert_eq!(cmd.log_groups, vec!["/a".to_string(), "b c".to_string()]);
    }

    #[test]
    fn selector_functions_are_reported_not_guessed() {
        let (cmd, _) = split(
            "SOURCE logGroups(namePrefix: ['/aws/lambda', '/x']) logGroupTags([{\"key\":\"t\"}]) \
             START=-1h | fields @timestamp",
        );
        assert!(cmd.log_groups.is_empty());
        assert_eq!(
            cmd.unresolved_selectors,
            vec!["logGroups".to_string(), "logGroupTags".to_string()]
        );
        assert_eq!(
            cmd.start,
            Some(SourceTime::Relative {
                offset_ms: -3_600_000
            })
        );
    }

    #[test]
    fn pipe_inside_quotes_or_brackets_does_not_end_the_command() {
        let (cmd, rest) = split("SOURCE \"/a|b\" logGroups(namePrefix: ['x|y']) | fields @x");
        assert_eq!(cmd.log_groups, vec!["/a|b".to_string()]);
        assert_eq!(cmd.unresolved_selectors, vec!["logGroups".to_string()]);
        assert_eq!(rest, "fields @x");
    }

    #[test]
    fn source_without_a_pipeline() {
        let (cmd, rest) = split("SOURCE \"/a\" START=-5m END=0s");
        assert_eq!(cmd.log_groups, vec!["/a".to_string()]);
        assert_eq!(rest, "");
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let (cmd, _) = split("source \"/a\" start=-1h end=0s | fields @x");
        assert_eq!(cmd.log_groups, vec!["/a".to_string()]);
        assert_eq!(
            cmd.start,
            Some(SourceTime::Relative {
                offset_ms: -3_600_000
            })
        );
    }

    #[test]
    fn absolute_epoch_and_rfc3339_bounds() {
        let (cmd, _) = split("SOURCE \"/a\" START=1700000000 END=1700003600000 | fields @x");
        assert_eq!(
            cmd.start,
            Some(SourceTime::Absolute {
                epoch_ms: 1_700_000_000_000
            })
        );
        assert_eq!(
            cmd.end,
            Some(SourceTime::Absolute {
                epoch_ms: 1_700_003_600_000
            })
        );

        let (cmd, _) = split("SOURCE \"/a\" START='2026-05-22T05:45:58.722Z' | fields @x");
        assert_eq!(
            cmd.start,
            Some(SourceTime::Absolute {
                epoch_ms: 1_779_428_758_722
            })
        );
    }

    #[test]
    fn rejects_unparseable_time() {
        let err = split_source_command("SOURCE \"/a\" START=last-tuesday | fields @x").unwrap_err();
        assert!(format!("{err}").contains("START"));
    }

    #[test]
    fn render_matches_the_console_form() {
        assert_eq!(
            prepend_source_command(
                &["/eks/prod/team-icc".to_string()],
                Some(SourceTime::Relative {
                    offset_ms: -604_800_000
                }),
                Some(SourceTime::Relative { offset_ms: 0 }),
                "fields @timestamp\n| limit 200",
            ),
            "SOURCE \"/eks/prod/team-icc\" START=-1w END=0s |\nfields @timestamp\n| limit 200"
        );
    }

    #[test]
    fn render_puts_each_log_group_in_its_own_source_command() {
        assert_eq!(
            prepend_source_command(
                &["/a".to_string(), "/b".to_string()],
                Some(SourceTime::Absolute {
                    epoch_ms: 1_700_000_000_000
                }),
                Some(SourceTime::Absolute {
                    epoch_ms: 1_700_003_600_000
                }),
                "fields @timestamp",
            ),
            "SOURCE \"/a\" START=1700000000000 END=1700003600000 |\nSOURCE \"/b\" |\nfields @timestamp"
        );
    }

    #[test]
    fn render_round_trips_through_the_parser() {
        let cases = [
            (
                vec!["/a".to_string(), "/b".to_string()],
                Some(SourceTime::Relative {
                    offset_ms: -1_800_000,
                }),
                Some(SourceTime::Relative { offset_ms: 0 }),
            ),
            (
                vec!["/a".to_string()],
                Some(SourceTime::Absolute {
                    epoch_ms: 1_700_000_000_000,
                }),
                Some(SourceTime::Absolute {
                    epoch_ms: 1_700_003_600_000,
                }),
            ),
            (vec!["/a".to_string()], None, None),
        ];
        for (log_groups, start, end) in cases {
            let query = "fields @timestamp\n| limit 20";
            let rendered = prepend_source_command(&log_groups, start, end, query);
            let (parsed, rest) = split(&rendered);
            assert_eq!(parsed.log_groups, log_groups, "{rendered}");
            assert_eq!(parsed.start, start, "{rendered}");
            assert_eq!(parsed.end, end, "{rendered}");
            assert_eq!(rest, query, "{rendered}");
        }
    }

    #[test]
    fn rejects_unterminated_quote() {
        assert!(split_source_command("SOURCE \"/a | fields @x").is_err());
    }
}
