use std::path::PathBuf;

use anyhow::Result;
use chrono::{TimeZone, Utc};

use crate::cli::PasteLinkArgs;
use crate::commands::fail;
use crate::console_link::{parse_console_link, parse_log_group_arn, RisonValue};
use crate::pasteboard::{read_from_pasteboard, PasteboardError};
use crate::paths;

pub async fn run(args: PasteLinkArgs) -> Result<()> {
    let url = resolve_url(&args).await?;
    let state = parse_link_to_state(&url)?;

    if args.as_raw {
        println!("{}", build_raw_command(&state));
        return Ok(());
    }

    let path = match args.output {
        Some(p) => PathBuf::from(p),
        None => {
            let cwd = std::env::current_dir()?;
            paths::current_insights_path(&cwd)
        }
    };
    write_insights_file(&path, &state)?;
    if !args.quiet {
        eprintln!("Wrote {}", path.display());
        eprintln!("  region:     {}", state.region);
        eprintln!("  log groups: {}", state.log_groups.join(", "));
        eprintln!("  time:       {}", state.time);
    }
    Ok(())
}

#[derive(Debug)]
pub struct ParsedLinkState {
    pub region: String,
    pub log_groups: Vec<String>,
    pub time: String,
    pub query: String,
}

pub fn parse_link_to_state(url: &str) -> Result<ParsedLinkState> {
    let parsed = parse_console_link(url)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let editor_string = parsed
        .query_detail
        .get("editorString")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("queryDetail.editorString is missing or not a string"))?
        .to_string();

    let source = parsed
        .query_detail
        .get("source")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("queryDetail.source is missing or empty"))?;
    if source.is_empty() {
        return Err(anyhow::anyhow!("queryDetail.source is missing or empty"));
    }
    let mut log_groups = Vec::with_capacity(source.len());
    for (i, entry) in source.iter().enumerate() {
        let s = entry
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(format!("queryDetail.source[{i}] is not a string")))?;
        if s.starts_with("arn:") {
            let parsed_arn = parse_log_group_arn(s).ok_or_else(|| {
                anyhow::anyhow!(format!("queryDetail.source[{i}] is not a log-group ARN: {s}"))
            })?;
            log_groups.push(parsed_arn.2);
        } else {
            log_groups.push(s.to_string());
        }
    }

    Ok(ParsedLinkState {
        region: parsed.region,
        log_groups,
        time: time_from_query_detail(&parsed.query_detail)?,
        query: editor_string,
    })
}

fn time_from_query_detail(detail: &indexmap::IndexMap<String, RisonValue>) -> Result<String> {
    let time_type = detail
        .get("timeType")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("queryDetail.timeType is missing"))?;
    if time_type == "RELATIVE" {
        let start = detail
            .get("start")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("RELATIVE time has non-numeric start"))?;
        let unit = detail.get("unit").and_then(|v| v.as_str()).unwrap_or("seconds");
        let seconds = if unit == "milliseconds" {
            (-start / 1000.0).round() as i64
        } else {
            -start as i64
        };
        return Ok(format_relative_duration(seconds));
    }
    if time_type == "ABSOLUTE" {
        let start = detail
            .get("start")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("ABSOLUTE time has non-numeric start/end"))?;
        let end = detail
            .get("end")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("ABSOLUTE time has non-numeric start/end"))?;
        return Ok(format!(
            "{}/{}",
            ms_to_iso(start as i64),
            ms_to_iso(end as i64)
        ));
    }
    Err(anyhow::anyhow!(format!("unknown timeType: {time_type:?}")))
}

fn ms_to_iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| ms.to_string())
}

/// Format a whole number of seconds as the largest exact unit understood
/// by the time-range parser.
pub fn format_relative_duration(total_seconds: i64) -> String {
    if total_seconds < 0 {
        return format!("{}s", total_seconds.abs());
    }
    if total_seconds == 0 {
        return "0s".into();
    }
    for (suffix, secs) in [
        ("w", 7 * 86_400),
        ("d", 86_400),
        ("h", 3_600),
        ("m", 60),
        ("s", 1),
    ] {
        if total_seconds % secs == 0 {
            return format!("{}{suffix}", total_seconds / secs);
        }
    }
    format!("{total_seconds}s")
}

fn write_insights_file(path: &std::path::Path, state: &ParsedLinkState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut fm = String::new();
    fm.push_str(&format!("time = {}\n", toml_string(&state.time)));
    if state.log_groups.len() == 1 {
        fm.push_str(&format!(
            "log-group = {}\n",
            toml_string(&state.log_groups[0])
        ));
    } else {
        let parts: Vec<String> = state.log_groups.iter().map(|s| toml_string(s)).collect();
        fm.push_str(&format!("log-group = [{}]\n", parts.join(", ")));
    }
    let contents = format!("{fm}\n---\n{}\n", state.query);
    std::fs::write(path, contents)?;
    Ok(())
}

fn toml_string(s: &str) -> String {
    let value = toml::Value::String(s.to_string());
    toml::to_string(&value).unwrap_or_else(|_| format!("\"{s}\""))
        .trim()
        .to_string()
}

/// Build a single-line shell command running the same query via `raw`,
/// piping the query body in via a quoted heredoc.
pub fn build_raw_command(state: &ParsedLinkState) -> String {
    let mut args: Vec<String> = vec!["cloudwatch-insights".into(), "raw".into()];
    args.push("-r".into());
    args.push(shell_quote(&state.region));
    for lg in &state.log_groups {
        args.push("-g".into());
        args.push(shell_quote(lg));
    }
    args.push("-t".into());
    args.push(shell_quote(&state.time));
    args.push("-f".into());
    args.push("-".into());
    format!(
        "{} <<'EOF'\n{}\nEOF",
        args.join(" "),
        state.query
    )
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    let safe = s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_./:@%+,=-".contains(&b));
    if safe {
        return s.to_string();
    }
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

async fn resolve_url(args: &PasteLinkArgs) -> Result<String> {
    if let Some(positional) = &args.positional_url {
        if args.prompt {
            return Err(fail(2, "--prompt cannot be combined with a positional URL"));
        }
        return Ok(positional.trim().to_string());
    }

    if args.prompt {
        return prompt_for_url();
    }

    let url = match read_from_pasteboard() {
        Ok(s) => s,
        Err(PasteboardError(msg)) => {
            return Err(fail(
                1,
                format!("{msg}\n  hint: pass the URL as a positional argument, or use --prompt"),
            ));
        }
    };
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(fail(
            2,
            "pasteboard is empty — pass the URL as a positional argument, or use --prompt",
        ));
    }
    Ok(url)
}

fn prompt_for_url() -> Result<String> {
    use std::io::{BufRead, Write};
    let mut stderr = std::io::stderr().lock();
    write!(stderr, "URL: ").map_err(|e| anyhow::anyhow!(e))?;
    stderr.flush().map_err(|e| anyhow::anyhow!(e))?;
    drop(stderr);

    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console_link::{
        build_console_link, build_query_detail, log_group_arn, ConsoleLinkInput,
        QueryDetailInput, RisonValue, TimeSpec,
    };

    fn build_link(
        region: &str,
        account_id: &str,
        log_groups: &[&str],
        query: &str,
        time: TimeSpec,
    ) -> String {
        let arns: Vec<String> = log_groups
            .iter()
            .map(|lg| log_group_arn(region, account_id, lg))
            .collect();
        let detail = build_query_detail(&QueryDetailInput {
            query,
            log_group_arns: &arns,
            time,
            query_id: None,
            log_class: None,
        });
        build_console_link(&ConsoleLinkInput {
            region,
            query_detail: &detail,
        })
    }

    #[test]
    fn format_relative_duration_picks_largest_unit() {
        assert_eq!(format_relative_duration(60), "1m");
        assert_eq!(format_relative_duration(3600), "1h");
        assert_eq!(format_relative_duration(86_400), "1d");
        assert_eq!(format_relative_duration(7 * 86_400), "1w");
        assert_eq!(format_relative_duration(2 * 3600), "2h");
        assert_eq!(format_relative_duration(90), "90s");
        assert_eq!(format_relative_duration(45), "45s");
        assert_eq!(format_relative_duration(0), "0s");
    }

    #[test]
    fn parse_link_relative_single_log_group() {
        let url = build_link(
            "eu-north-1",
            "123456789012",
            &["/my/service/logs"],
            "fields @timestamp, @message\n| limit 200",
            TimeSpec::Relative { seconds_back: 3600 },
        );
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.region, "eu-north-1");
        assert_eq!(state.log_groups, vec!["/my/service/logs".to_string()]);
        assert_eq!(state.time, "1h");
        assert_eq!(state.query, "fields @timestamp, @message\n| limit 200");
    }

    #[test]
    fn parse_link_absolute_emits_iso_range() {
        let url = build_link(
            "eu-north-1",
            "1",
            &["/a", "/b"],
            "fields @timestamp",
            TimeSpec::Absolute {
                start_ms: 1700000000000,
                end_ms: 1700003600000,
            },
        );
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.log_groups, vec!["/a".to_string(), "/b".to_string()]);
        let start_iso = ms_to_iso(1700000000000);
        let end_iso = ms_to_iso(1700003600000);
        assert_eq!(state.time, format!("{start_iso}/{end_iso}"));
    }

    #[test]
    fn parse_link_accepts_bare_log_group_names() {
        let arns: Vec<String> = vec![];
        let mut detail = build_query_detail(&QueryDetailInput {
            query: "fields @timestamp",
            log_group_arns: &arns,
            time: TimeSpec::Relative { seconds_back: 60 },
            query_id: None,
            log_class: None,
        });
        detail.insert(
            "source".into(),
            RisonValue::Array(vec![RisonValue::String("/my/service/logs".into())]),
        );
        let url = build_console_link(&ConsoleLinkInput {
            region: "eu-north-1",
            query_detail: &detail,
        });
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.log_groups, vec!["/my/service/logs".to_string()]);
        assert_eq!(state.region, "eu-north-1");
    }

    #[test]
    fn parse_link_rejects_url_without_log_groups() {
        let arns = vec!["arn:aws:logs:eu-north-1:1:log-group:/x".to_string()];
        let mut detail = build_query_detail(&QueryDetailInput {
            query: "fields @timestamp",
            log_group_arns: &arns,
            time: TimeSpec::Relative { seconds_back: 60 },
            query_id: None,
            log_class: None,
        });
        detail.shift_remove("source");
        let url = build_console_link(&ConsoleLinkInput {
            region: "eu-north-1",
            query_detail: &detail,
        });
        let err = parse_link_to_state(&url).unwrap_err();
        assert!(format!("{err}").contains("source"));
    }

    #[test]
    fn raw_command_basic() {
        let cmd = build_raw_command(&ParsedLinkState {
            region: "eu-north-1".into(),
            log_groups: vec!["/my/service/logs".into()],
            time: "1h".into(),
            query: "fields @timestamp\n| limit 200".into(),
        });
        assert_eq!(
            cmd,
            "cloudwatch-insights raw -r eu-north-1 -g /my/service/logs -t 1h -f - <<'EOF'\n\
             fields @timestamp\n| limit 200\nEOF"
        );
    }

    #[test]
    fn raw_command_quotes_special_chars() {
        let cmd = build_raw_command(&ParsedLinkState {
            region: "eu-north-1".into(),
            log_groups: vec!["/with space".into(), "/another".into()],
            time: "2026-04-22T13:00:00Z/2026-04-22T14:00:00Z".into(),
            query: "fields @timestamp".into(),
        });
        assert!(cmd.contains("-g '/with space' -g /another "));
        assert!(cmd.contains("-t 2026-04-22T13:00:00Z/2026-04-22T14:00:00Z "));
    }

    #[test]
    fn raw_command_escapes_single_quotes() {
        let cmd = build_raw_command(&ParsedLinkState {
            region: "x".into(),
            log_groups: vec!["/it's".into()],
            time: "1h".into(),
            query: "q".into(),
        });
        assert!(cmd.contains("-g '/it'\\''s'"));
    }
}
