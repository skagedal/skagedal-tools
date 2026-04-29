use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_sdk_cloudwatchlogs::Client;
use chrono::Local;
use tokio::io::AsyncReadExt;

use crate::cli::RawArgs;
use crate::commands::fail;
use crate::insights::{run_insights_query, RunQueryOptions, StatusCallback};
use crate::output::write_results_strings;
use crate::time_range::{parse_time_range, TimeRangeParseError};

pub async fn run(args: RawArgs) -> Result<()> {
    let query_file = args
        .query_file
        .as_deref()
        .ok_or_else(|| fail(2, "--query-file/-f is required"))?;
    let log_groups: Vec<String> = args
        .log_group
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if log_groups.is_empty() {
        return Err(fail(
            2,
            "--log-group/-g is required (repeat or comma-separate)",
        ));
    }

    let query_string = read_query_source(query_file).await?;

    let time_expr = args.time.as_deref().unwrap_or("1h");
    let range = parse_time_range(time_expr, Local::now())
        .map_err(|e: TimeRangeParseError| fail(2, e.to_string()))?;

    if let Some(profile) = &args.profile {
        // SAFETY: process-wide env mutation; this CLI is single-threaded for
        // its setup phase and the SDK reads it once we configure it below.
        unsafe {
            std::env::set_var("AWS_PROFILE", profile);
        }
    }

    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(region) = &args.region {
        loader = loader.region(aws_config::Region::new(region.clone()));
    }
    let config = loader.load().await;
    let client = Client::new(&config);

    if !args.quiet {
        eprintln!(
            "Querying {n} log group(s) from {start} to {end}",
            n = log_groups.len(),
            start = range.start.to_rfc3339(),
            end = range.end.to_rfc3339(),
        );
        eprintln!("  log groups: {}", log_groups.join(", "));
    }

    let mut last_status = String::new();
    let quiet = args.quiet;
    let on_status: Option<StatusCallback> = if quiet {
        None
    } else {
        Some(Box::new(move |status: &str| {
            if status == last_status {
                return;
            }
            last_status = status.to_string();
            eprintln!("  status: {status}");
        }))
    };

    let result = run_insights_query(RunQueryOptions {
        client: &client,
        log_groups: &log_groups,
        query_string: &query_string,
        start_time: range.start,
        end_time: range.end,
        limit: args.limit,
        poll_interval: Duration::from_secs(1),
        on_status,
    })
    .await?;

    if let Some(output_path) = args.output.as_deref() {
        let path = PathBuf::from(output_path);
        write_results_strings(&path, &result.rows)?;
    } else {
        let mut buf = String::new();
        for row in &result.rows {
            let map: serde_json::Map<String, serde_json::Value> = row
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            buf.push_str(&serde_json::to_string(&serde_json::Value::Object(map))?);
            buf.push('\n');
        }
        print!("{buf}");
    }

    if !args.quiet {
        let stats = result.statistics.unwrap_or_default();
        eprintln!(
            "Done. {n} rows written (matched={m} scanned={s} bytes={b}).",
            n = result.rows.len(),
            m = stats
                .records_matched
                .map(|v| (v as i64).to_string())
                .unwrap_or_else(|| "?".into()),
            s = stats
                .records_scanned
                .map(|v| (v as i64).to_string())
                .unwrap_or_else(|| "?".into()),
            b = stats
                .bytes_scanned
                .map(|v| (v as i64).to_string())
                .unwrap_or_else(|| "?".into()),
        );
    }

    Ok(())
}

async fn read_query_source(source: &str) -> Result<String> {
    if source == "-" {
        let mut buf = String::new();
        tokio::io::stdin().read_to_string(&mut buf).await?;
        Ok(buf)
    } else {
        Ok(tokio::fs::read_to_string(source).await?)
    }
}
