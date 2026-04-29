//! CloudWatch Logs Insights runtime: start_query → poll → results.

use anyhow::{anyhow, Result};
use aws_sdk_cloudwatchlogs::Client;
use aws_sdk_cloudwatchlogs::types::QueryStatus;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use std::time::Duration;

pub type QueryRow = IndexMap<String, String>;

/// Streaming status callback used by [`run_insights_query`].
pub type StatusCallback = Box<dyn FnMut(&str) + Send>;

#[derive(Debug, Default)]
pub struct QueryStatistics {
    pub records_matched: Option<f64>,
    pub records_scanned: Option<f64>,
    pub bytes_scanned: Option<f64>,
}

#[derive(Debug)]
pub struct QueryResult {
    pub rows: Vec<QueryRow>,
    pub statistics: Option<QueryStatistics>,
}

pub struct RunQueryOptions<'a> {
    pub client: &'a Client,
    pub log_groups: &'a [String],
    pub query_string: &'a str,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub limit: Option<i32>,
    pub poll_interval: Duration,
    pub on_status: Option<StatusCallback>,
}

pub async fn run_insights_query(mut options: RunQueryOptions<'_>) -> Result<QueryResult> {
    if options.log_groups.is_empty() {
        return Err(anyhow!("at least one log group is required"));
    }

    let start_resp = options
        .client
        .start_query()
        .set_log_group_names(Some(options.log_groups.to_vec()))
        .start_time(options.start_time.timestamp())
        .end_time(options.end_time.timestamp())
        .query_string(options.query_string)
        .set_limit(options.limit)
        .send()
        .await
        .map_err(|e| anyhow!("StartQuery failed: {}", display_sdk_err(&e)))?;

    let query_id = start_resp
        .query_id
        .ok_or_else(|| anyhow!("StartQuery did not return a queryId"))?;

    loop {
        let resp = options
            .client
            .get_query_results()
            .query_id(&query_id)
            .send()
            .await
            .map_err(|e| anyhow!("GetQueryResults failed: {}", display_sdk_err(&e)))?;

        let status = resp.status.clone().unwrap_or(QueryStatus::UnknownValue);
        let status_str = status_label(&status);
        if let Some(cb) = options.on_status.as_mut() {
            cb(status_str);
        }

        if is_terminal(&status) {
            if status != QueryStatus::Complete {
                return Err(anyhow!("query ended with status: {status_str}"));
            }
            let rows: Vec<QueryRow> = resp
                .results
                .unwrap_or_default()
                .into_iter()
                .map(result_fields_to_row)
                .collect();
            let statistics = resp.statistics.map(|s| QueryStatistics {
                records_matched: Some(s.records_matched),
                records_scanned: Some(s.records_scanned),
                bytes_scanned: Some(s.bytes_scanned),
            });
            return Ok(QueryResult { rows, statistics });
        }

        tokio::time::sleep(options.poll_interval).await;
    }
}

fn status_label(status: &QueryStatus) -> &'static str {
    match status {
        QueryStatus::Scheduled => "Scheduled",
        QueryStatus::Running => "Running",
        QueryStatus::Complete => "Complete",
        QueryStatus::Failed => "Failed",
        QueryStatus::Cancelled => "Cancelled",
        QueryStatus::Timeout => "Timeout",
        QueryStatus::UnknownValue => "Unknown",
        _ => "Unknown",
    }
}

fn is_terminal(status: &QueryStatus) -> bool {
    matches!(
        status,
        QueryStatus::Complete
            | QueryStatus::Failed
            | QueryStatus::Cancelled
            | QueryStatus::Timeout
            | QueryStatus::UnknownValue
    )
}

fn result_fields_to_row(
    fields: Vec<aws_sdk_cloudwatchlogs::types::ResultField>,
) -> QueryRow {
    let mut row = IndexMap::new();
    for f in fields {
        let Some(name) = f.field else { continue };
        row.insert(name, f.value.unwrap_or_default());
    }
    row
}

fn display_sdk_err<E: std::fmt::Display + std::error::Error>(e: &E) -> String {
    let mut out = format!("{e}");
    let mut src: Option<&(dyn std::error::Error + 'static)> = e.source();
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&format!("{s}"));
        src = s.source();
    }
    out
}
