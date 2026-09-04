use std::path::PathBuf;

use anyhow::Result;
use chrono::{TimeZone, Utc};

use crate::cli::PasteLinkArgs;
use crate::commands::fail;
use crate::console_link::{
    LinkDetail, LogAnalyticsTab, RisonValue, parse_console_link, parse_log_group_arn,
};
use crate::pasteboard::{PasteboardError, read_from_pasteboard};
use crate::paths;
use crate::source_command::{SourceCommand, SourceTime, split_source_command};
use crate::time_range::format_duration_ms;

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
        match &state.time {
            Some(time) => eprintln!("  time:       {time}"),
            None => eprintln!("  time:       (none in URL — the 1h default applies)"),
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct ParsedLinkState {
    pub region: String,
    pub log_groups: Vec<String>,
    /// `None` when the URL carries no time window, in which case the tool's
    /// own 1h default applies.
    pub time: Option<String>,
    pub query: String,
}

pub fn parse_link_to_state(url: &str) -> Result<ParsedLinkState> {
    let parsed = parse_console_link(url).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    match parsed.detail {
        LinkDetail::QueryDetail(detail) => state_from_query_detail(parsed.region, detail),
        LinkDetail::LogAnalytics(tab) => state_from_log_analytics(parsed.region, tab),
    }
}

/// Build the state from a revamped "Log Analytics" link, where the log groups
/// and the time window live in the query's leading `SOURCE ...` command.
fn state_from_log_analytics(region: String, tab: LogAnalyticsTab) -> Result<ParsedLinkState> {
    if let Some(kind) = &tab.kind {
        if kind != "query" {
            eprintln!(
                "warning: the URL's tab has type {kind:?}, not \"query\" — \
                 the query body may not be Logs Insights QL"
            );
        }
    }

    let (source, query) = split_source_command(&tab.query)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the URL's query has no leading SOURCE command, so it names no log groups"
            )
        })?;

    if source.log_groups.is_empty() {
        let hint = if source.unresolved_selectors.is_empty() {
            String::new()
        } else {
            format!(
                " — it selects them with {}(...), which only CloudWatch can resolve",
                source.unresolved_selectors.join("(...), ")
            )
        };
        return Err(anyhow::anyhow!(format!(
            "the URL's SOURCE command names no log groups explicitly{hint}"
        )));
    }

    let time = time_from_source_command(&source);
    Ok(ParsedLinkState {
        region,
        log_groups: source.log_groups,
        time,
        query,
    })
}

/// Turn the `SOURCE`'s `START`/`END` into the tool's time-range syntax: a
/// relative duration when the window ends at "now", an ISO range otherwise.
fn time_from_source_command(source: &SourceCommand) -> Option<String> {
    let start = source.start?;
    let end = source.end.unwrap_or(SourceTime::Relative { offset_ms: 0 });
    if let (SourceTime::Relative { offset_ms: back }, SourceTime::Relative { offset_ms: 0 }) =
        (start, end)
    {
        return Some(format_relative_duration(-back / 1000));
    }
    let now_ms = Utc::now().timestamp_millis();
    let resolve = |t: SourceTime| match t {
        SourceTime::Relative { offset_ms } => now_ms + offset_ms,
        SourceTime::Absolute { epoch_ms } => epoch_ms,
    };
    Some(format!(
        "{}/{}",
        ms_to_iso(resolve(start)),
        ms_to_iso(resolve(end))
    ))
}

/// Build the state from a classic Logs Insights link, where the log groups and
/// the time window are fields of the URL's `queryDetail` object.
fn state_from_query_detail(
    region: String,
    query_detail: indexmap::IndexMap<String, RisonValue>,
) -> Result<ParsedLinkState> {
    let editor_string = query_detail
        .get("editorString")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("queryDetail.editorString is missing or not a string"))?
        .to_string();

    let source = query_detail
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
                anyhow::anyhow!(format!(
                    "queryDetail.source[{i}] is not a log-group ARN: {s}"
                ))
            })?;
            log_groups.push(parsed_arn.2);
        } else {
            log_groups.push(s.to_string());
        }
    }

    Ok(ParsedLinkState {
        region,
        log_groups,
        time: Some(time_from_query_detail(&query_detail)?),
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
        let unit = detail
            .get("unit")
            .and_then(|v| v.as_str())
            .unwrap_or("seconds");
        let seconds = if unit == "milliseconds" {
            (-start / 1000.0).round() as i64
        } else {
            -start as i64
        };
        return Ok(format_relative_duration(seconds));
    }
    if time_type == "ABSOLUTE" {
        let start = absolute_bound_ms(detail.get("start"), "start")?;
        let end = absolute_bound_ms(detail.get("end"), "end")?;
        return Ok(format!("{}/{}", ms_to_iso(start), ms_to_iso(end)));
    }
    Err(anyhow::anyhow!(format!("unknown timeType: {time_type:?}")))
}

fn absolute_bound_ms(value: Option<&RisonValue>, field: &str) -> Result<i64> {
    let value =
        value.ok_or_else(|| anyhow::anyhow!(format!("ABSOLUTE time is missing {field}")))?;
    if let Some(n) = value.as_f64() {
        return Ok(n as i64);
    }
    if let Some(s) = value.as_str() {
        let dt = chrono::DateTime::parse_from_rfc3339(s).map_err(|e| {
            anyhow::anyhow!(format!(
                "ABSOLUTE time {field} is not a number or RFC 3339 string ({s:?}): {e}"
            ))
        })?;
        return Ok(dt.timestamp_millis());
    }
    Err(anyhow::anyhow!(format!(
        "ABSOLUTE time {field} is not a number or string"
    )))
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
    format_duration_ms(total_seconds * 1000)
}

fn write_insights_file(path: &std::path::Path, state: &ParsedLinkState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut fm = String::new();
    if let Some(time) = &state.time {
        fm.push_str(&format!("time = {}\n", toml_string(time)));
    }
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
    toml::to_string(&value)
        .unwrap_or_else(|_| format!("\"{s}\""))
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
    if let Some(time) = &state.time {
        args.push("-t".into());
        args.push(shell_quote(time));
    }
    args.push("-f".into());
    args.push("-".into());
    format!("{} <<'EOF'\n{}\nEOF", args.join(" "), state.query)
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
    use crate::console_link::encode_aws_string;

    const RELATIVE_1H: &str = "end~0~start~-3600~timeType~'RELATIVE~tz~'UTC~unit~'seconds";
    const ABSOLUTE: &str = "end~1700003600000~start~1700000000000~timeType~'ABSOLUTE\
                            ~tz~'UTC~unit~'milliseconds";

    /// A classic Logs Insights URL, as the console produced them before the
    /// "Log Analytics" revamp. The tool still reads these but no longer writes
    /// them, so the tests spell the format out rather than round-tripping.
    fn classic_url(time: &str, sources: &[&str], query: &str) -> String {
        let source = if sources.is_empty() {
            String::new()
        } else {
            let encoded: Vec<String> = sources
                .iter()
                .map(|s| format!("'{}", encode_aws_string(s)))
                .collect();
            format!("~source~(~{})", encoded.join("~"))
        };
        format!(
            "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1\
             #logsV2:logs-insights$3FqueryDetail$3D~({time}~editorString~'{query}{source}\
             ~lang~'CWLI~logClass~'STANDARD~queryBy~'logGroupName)",
            query = encode_aws_string(query),
        )
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
        let url = classic_url(
            RELATIVE_1H,
            &["arn:aws:logs:eu-north-1:123456789012:log-group:/my/service/logs"],
            "fields @timestamp, @message\n| limit 200",
        );
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.region, "eu-north-1");
        assert_eq!(state.log_groups, vec!["/my/service/logs".to_string()]);
        assert_eq!(state.time.as_deref(), Some("1h"));
        assert_eq!(state.query, "fields @timestamp, @message\n| limit 200");
    }

    #[test]
    fn parse_link_absolute_emits_iso_range() {
        let url = classic_url(
            ABSOLUTE,
            &[
                "arn:aws:logs:eu-north-1:1:log-group:/a",
                "arn:aws:logs:eu-north-1:1:log-group:/b",
            ],
            "fields @timestamp",
        );
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.log_groups, vec!["/a".to_string(), "/b".to_string()]);
        let start_iso = ms_to_iso(1700000000000);
        let end_iso = ms_to_iso(1700003600000);
        assert_eq!(state.time, Some(format!("{start_iso}/{end_iso}")));
    }

    #[test]
    fn parse_link_absolute_accepts_iso_string_bounds() {
        // The AWS Console sometimes encodes ABSOLUTE start/end as ISO-8601
        // strings instead of epoch-ms numbers. Make sure we accept that form.
        let url = "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1\
            #logsV2:logs-insights$3FqueryDetail$3D~(end~'2026-05-22T11*3a33*3a14.813Z\
            ~start~'2026-05-22T05*3a45*3a58.722Z~timeType~'ABSOLUTE~tz~'UTC\
            ~editorString~'fields*20*40timestamp\
            ~source~(~'arn*3aaws*3alogs*3aeu-north-1*3a1*3alog-group*3a*2fx)\
            ~lang~'CWLI~logClass~'STANDARD~queryBy~'logGroupName)";
        let state = parse_link_to_state(url).unwrap();
        assert_eq!(state.region, "eu-north-1");
        assert_eq!(state.log_groups, vec!["/x".to_string()]);
        assert_eq!(
            state.time.as_deref(),
            Some("2026-05-22T05:45:58.722Z/2026-05-22T11:33:14.813Z")
        );
    }

    #[test]
    fn parse_link_accepts_bare_log_group_names() {
        let url = classic_url(RELATIVE_1H, &["/my/service/logs"], "fields @timestamp");
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.log_groups, vec!["/my/service/logs".to_string()]);
        assert_eq!(state.region, "eu-north-1");
    }

    #[test]
    fn parse_link_rejects_url_without_log_groups() {
        let url = classic_url(RELATIVE_1H, &[], "fields @timestamp");
        let err = parse_link_to_state(&url).unwrap_err();
        assert!(format!("{err}").contains("source"));
    }

    #[test]
    fn parse_link_rejects_unknown_time_type() {
        let url = classic_url(
            "end~0~start~-3600~timeType~'SOMEDAY",
            &["/a"],
            "fields @timestamp",
        );
        let err = parse_link_to_state(&url).unwrap_err();
        assert!(format!("{err}").contains("SOMEDAY"));
    }

    /// The revamped console keeps the log groups and the time window in the
    /// query's `SOURCE` command instead of beside it.
    fn log_analytics_url(query: &str) -> String {
        format!(
            "https://eu-north-1.console.aws.amazon.com/cloudwatch/home?region=eu-north-1\
             #log-analytics?active=%7E%27a&a.type=%7E%27query&a.query=%7E%27{}",
            crate::console_link::encode_aws_string(query)
        )
    }

    #[test]
    fn parse_log_analytics_link_relative_window() {
        let url = log_analytics_url(
            "SOURCE \"/eks/prod/team-icc\" START=-1w END=0s |\nfields @timestamp\n| limit 200",
        );
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.region, "eu-north-1");
        assert_eq!(state.log_groups, vec!["/eks/prod/team-icc".to_string()]);
        assert_eq!(state.time.as_deref(), Some("1w"));
        assert_eq!(state.query, "fields @timestamp\n| limit 200");
    }

    #[test]
    fn parse_log_analytics_link_several_log_groups() {
        let url = log_analytics_url("SOURCE \"/a\", \"/b\" START=-30m END=0s | fields @timestamp");
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.log_groups, vec!["/a".to_string(), "/b".to_string()]);
        assert_eq!(state.time.as_deref(), Some("30m"));
    }

    #[test]
    fn parse_log_analytics_link_absolute_window() {
        let url = log_analytics_url(
            "SOURCE \"/a\" START=1700000000000 END=1700003600000 | fields @timestamp",
        );
        let state = parse_link_to_state(&url).unwrap();
        let start_iso = ms_to_iso(1700000000000);
        let end_iso = ms_to_iso(1700003600000);
        assert_eq!(state.time, Some(format!("{start_iso}/{end_iso}")));
    }

    #[test]
    fn parse_log_analytics_link_without_a_time_window() {
        let url = log_analytics_url("SOURCE \"/a\" | fields @timestamp");
        let state = parse_link_to_state(&url).unwrap();
        assert_eq!(state.time, None);
        // No `time` line, so the tool's own 1h default applies.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.insights");
        write_insights_file(&path, &state).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("time ="), "{written}");
        assert!(written.contains("log-group = \"/a\""), "{written}");
        assert!(!build_raw_command(&state).contains(" -t "));
    }

    #[test]
    fn parse_log_analytics_link_rejects_unresolvable_log_groups() {
        let url = log_analytics_url(
            "SOURCE logGroups(namePrefix: ['/eks/prod']) START=-1h END=0s | fields @timestamp",
        );
        let err = parse_link_to_state(&url).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("logGroups(...)"), "{msg}");
    }

    #[test]
    fn parse_log_analytics_link_rejects_query_without_source() {
        let url = log_analytics_url("fields @timestamp | limit 20");
        let err = parse_link_to_state(&url).unwrap_err();
        assert!(format!("{err}").contains("SOURCE"));
    }

    #[test]
    fn raw_command_basic() {
        let cmd = build_raw_command(&ParsedLinkState {
            region: "eu-north-1".into(),
            log_groups: vec!["/my/service/logs".into()],
            time: Some("1h".into()),
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
            time: Some("2026-04-22T13:00:00Z/2026-04-22T14:00:00Z".into()),
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
            time: Some("1h".into()),
            query: "q".into(),
        });
        assert!(cmd.contains("-g '/it'\\''s'"));
    }
}
