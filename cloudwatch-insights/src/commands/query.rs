use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_sdk_cloudwatchlogs::Client;
use chrono::{Local, Utc};

use crate::cli::QueryArgs;
use crate::commands::fail;
use crate::config::{
    RepoConfig, is_valid_environment_name, load_settings, resolve_env_config, resolve_repo_defaults,
};
use crate::console_link::{
    ConsoleLinkInput, QueryDetailInput, TimeSpec, build_console_link, build_query_detail,
};
use crate::editor::open_editor;
use crate::flatten::flatten_row;
use crate::insights::{ProgressCallback, QueryProgress, RunQueryOptions, run_insights_query};
use crate::output::{update_latest_symlink, write_results};
use crate::paths;
use crate::progress::{HEADER_COLUMN_WIDTH, ProgressReporter};
use crate::query_file::{
    FrontMatter, LogGroupValue, SeedFrontMatter, SeedOptions, ensure_current_insights,
    load_query_file, parse_query_file, reset_current_insights, run_timestamp,
};
use crate::template::expand_template;
use crate::terminal::{Color, ColorOptions, colorize, styled_link};
use crate::time_range::{
    TimeRange, TimeRangeParseError, parse_time_range, try_parse_relative_duration_seconds,
};

pub async fn run(args: QueryArgs) -> Result<()> {
    if args.query.is_some() && args.query_file.is_some() {
        return Err(fail(2, "--query and --query-file are mutually exclusive"));
    }

    let cwd = std::env::current_dir()?;
    let settings_path = paths::config_path();
    let settings = load_settings(&settings_path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let (section_name, defaults) = resolve_repo_defaults(&settings, &cwd);

    let cli_environment = validate_environment_arg(args.environment.as_deref(), "--environment")?;

    let (query_body, front_matter) =
        resolve_query_source(&args, &defaults, cli_environment.as_deref()).await?;

    let environment = match cli_environment {
        Some(e) => Some(e),
        None => validate_environment_arg(front_matter.env.as_deref(), "front-matter `env`")?,
    };
    let time_expr = args
        .time
        .clone()
        .or_else(|| front_matter.time.clone())
        .unwrap_or_else(|| "1h".into());
    let limit = args.limit;

    let range = parse_time_range(&time_expr, Local::now())
        .map_err(|e: TimeRangeParseError| fail(2, e.to_string()))?;

    let app = front_matter
        .app
        .as_deref()
        .or(defaults.app.as_deref())
        .map(|s| s.to_string());

    let mut template_vars: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    template_vars.insert("env", environment.as_deref());
    template_vars.insert("app", app.as_deref());

    let log_groups = resolve_log_groups(
        &cli_log_groups(&args.log_group),
        front_matter.log_group.as_ref(),
        defaults.group.as_deref(),
        &template_vars,
        section_name.as_deref(),
    )?;

    let expanded_query =
        expand_template(&query_body, &template_vars).map_err(|e| fail(2, e.to_string()))?;

    let env_config = match environment.as_deref() {
        Some(e) => resolve_env_config(&settings, section_name.as_deref(), e),
        None => Default::default(),
    };
    let profile = args
        .profile
        .clone()
        .or_else(|| env_config.aws_profile.clone());
    let region = args
        .region
        .clone()
        .or_else(|| env_config.region.clone())
        .or_else(|| defaults.region.clone())
        .or_else(|| std::env::var("AWS_REGION").ok());

    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(p) = &profile {
        loader = loader.profile_name(p.clone());
    }
    if let Some(r) = &region {
        loader = loader.region(aws_config::Region::new(r.clone()));
    }
    let aws_config = loader.load().await;
    let client = Client::new(&aws_config);

    if !args.quiet {
        print_header(region.as_deref(), &log_groups, &time_expr);
    }

    let console_url = build_query_console_url(
        region.as_deref(),
        &log_groups,
        &expanded_query,
        &time_expr,
        &range,
    );

    if front_matter.dry == Some(true) {
        if !args.quiet {
            eprintln!();
            eprintln!("Dry run (front-matter `dry = true`); not contacting AWS.");
        }
        emit_trailer(console_url.as_deref(), args.quiet);
        return Ok(());
    }

    let quiet = args.quiet;
    let reporter_outer = std::sync::Arc::new(std::sync::Mutex::new(ProgressReporter::new(quiet)));
    let reporter_for_cb = reporter_outer.clone();
    let on_progress: Option<ProgressCallback> = if quiet {
        None
    } else {
        Some(Box::new(move |progress: &QueryProgress| {
            if let Ok(mut r) = reporter_for_cb.lock() {
                r.update(progress);
            }
        }))
    };

    let result = run_insights_query(RunQueryOptions {
        client: &client,
        log_groups: &log_groups,
        query_string: &expanded_query,
        start_time: range.start,
        end_time: range.end,
        limit,
        poll_interval: Duration::from_secs(1),
        on_progress,
    })
    .await?;

    if let Ok(mut r) = reporter_outer.lock() {
        r.done();
    }

    let flatten_fields = defaults.flatten_fields.clone().unwrap_or_default();
    let rows: Vec<_> = result
        .rows
        .iter()
        .map(|r| flatten_row(r, &flatten_fields))
        .collect();

    let out_path = run_result_path(&cwd);
    write_results(&out_path, &rows)?;
    let _link_path = update_latest_symlink(&out_path)?;

    emit_trailer(console_url.as_deref(), args.quiet);
    Ok(())
}

fn print_header(region: Option<&str>, log_groups: &[String], time_expr: &str) {
    print_header_line("AWS region:", region.unwrap_or("(unset)"));
    print_header_line("Log groups:", &log_groups.join(", "));
    print_header_line("Time range:", time_expr);
}

fn print_header_line(label: &str, value: &str) {
    let bold = colorize(
        label,
        ColorOptions {
            bold: true,
            ..Default::default()
        },
    );
    let pad = " ".repeat(HEADER_COLUMN_WIDTH.saturating_sub(label.len()));
    eprintln!("{bold}{pad}{value}");
}

fn emit_trailer(console_url: Option<&str>, quiet: bool) {
    if quiet {
        if let Some(url) = console_url {
            eprintln!("{url}");
        }
        return;
    }
    use std::io::IsTerminal;
    eprintln!();
    let link = match console_url {
        Some(url) if std::io::stderr().is_terminal() => {
            let style = ColorOptions {
                color: Some(Color::Cyan),
                underline: false,
                bold: false,
            };
            Some(styled_link(url, "Open in AWS", style))
        }
        Some(url) => Some(format!("Open in AWS: {url}")),
        None => None,
    };
    match link {
        Some(l) => eprintln!("Use cloudwatch-insights show to view results. ({l})"),
        None => eprintln!("Use cloudwatch-insights show to view results."),
    }
}

/// Build a shareable AWS Console URL for the in-progress query, using bare
/// log-group names (queryBy=logGroupName) so we don't have to look up an
/// account ID. Returns None when no region is known.
fn build_query_console_url(
    region: Option<&str>,
    log_groups: &[String],
    query: &str,
    time_expr: &str,
    range: &TimeRange,
) -> Option<String> {
    let region = region?;
    let time = pick_time_spec(time_expr, range);
    let log_group_arns: Vec<String> = log_groups.to_vec();
    let detail = build_query_detail(&QueryDetailInput {
        query,
        log_group_arns: &log_group_arns,
        time,
        query_id: None,
        log_class: None,
    });
    Some(build_console_link(&ConsoleLinkInput {
        region,
        query_detail: &detail,
    }))
}

fn pick_time_spec(time_expr: &str, range: &TimeRange) -> TimeSpec {
    if let Some(seconds) = try_parse_relative_duration_seconds(time_expr) {
        return TimeSpec::Relative {
            seconds_back: seconds,
        };
    }
    TimeSpec::Absolute {
        start_ms: range.start.timestamp_millis(),
        end_ms: range.end.timestamp_millis(),
    }
}

fn cli_log_groups(raw: &[String]) -> Vec<String> {
    // clap's value_delimiter already split on commas. Trim and drop blanks.
    raw.iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn validate_environment_arg(raw: Option<&str>, source: &str) -> Result<Option<String>> {
    match raw {
        None => Ok(None),
        Some(v) => {
            if !is_valid_environment_name(v) {
                Err(fail(
                    2,
                    format!(
                        "{source} must be lower kebab-case (e.g. \"prod\", \"us-east-1\") — got {v:?}"
                    ),
                ))
            } else {
                Ok(Some(v.to_string()))
            }
        }
    }
}

fn resolve_log_groups(
    cli_log_group: &[String],
    front_matter_log_group: Option<&LogGroupValue>,
    config_group_template: Option<&str>,
    template_vars: &BTreeMap<&str, Option<&str>>,
    section_name: Option<&str>,
) -> Result<Vec<String>> {
    let expand = |templates: Vec<String>| -> Result<Vec<String>> {
        let mut out = Vec::with_capacity(templates.len());
        for t in templates {
            let v = expand_template(&t, template_vars).map_err(|e| fail(2, e.to_string()))?;
            out.push(v);
        }
        Ok(out)
    };

    let from_cli: Vec<String> = cli_log_group
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !from_cli.is_empty() {
        return expand(from_cli);
    }
    if let Some(lg) = front_matter_log_group {
        let v = lg.to_vec();
        if !v.is_empty() {
            return expand(v);
        }
    }
    if let Some(t) = config_group_template {
        return expand(vec![t.to_string()]);
    }
    Err(fail(
        2,
        match section_name {
            Some(name) => format!(
                "no log group given, and config section [{name}] has no \"group\". \
                 Set it with `cloudwatch-insights edit-config` or pass --log-group."
            ),
            None => "no log group given, and no matching config section was found. \
                Pass --log-group or run `cloudwatch-insights edit-config`."
                .into(),
        },
    ))
}

async fn resolve_query_source(
    args: &QueryArgs,
    defaults: &RepoConfig,
    cli_environment: Option<&str>,
) -> Result<(String, FrontMatter)> {
    if let Some(q) = &args.query {
        return Ok((q.clone(), FrontMatter::default()));
    }
    if let Some(qf) = &args.query_file {
        let raw = if qf == "-" {
            use tokio::io::AsyncReadExt;
            let mut s = String::new();
            tokio::io::stdin().read_to_string(&mut s).await?;
            s
        } else {
            tokio::fs::read_to_string(qf).await?
        };
        let parsed = parse_query_file(&raw).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        return Ok((parsed.body, parsed.front_matter));
    }

    let cwd = std::env::current_dir()?;
    let path = paths::current_insights_path(&cwd);
    let seed = SeedOptions {
        app: defaults.app.as_deref(),
        front_matter: build_seed_front_matter(args, defaults, cli_environment),
    };
    if args.new {
        reset_current_insights(&path, &seed).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if !args.quiet {
            eprintln!("Reset {} to default template", path.display());
        }
    } else {
        let seeded =
            ensure_current_insights(&path, &seed).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if seeded && !args.quiet {
            eprintln!("Seeded {}", path.display());
        }
    }
    open_editor(&path)?;
    let parsed = load_query_file(&path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if parsed.body.is_empty() {
        return Err(fail(2, format!("{} has no query body", path.display())));
    }
    Ok((parsed.body, parsed.front_matter))
}

fn build_seed_front_matter(
    args: &QueryArgs,
    defaults: &RepoConfig,
    cli_environment: Option<&str>,
) -> SeedFrontMatter {
    let cli: Vec<String> = args
        .log_group
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let log_group = if cli.len() == 1 {
        Some(LogGroupValue::Single(cli.into_iter().next().unwrap()))
    } else if cli.len() > 1 {
        Some(LogGroupValue::Multiple(cli))
    } else {
        defaults.group.clone().map(LogGroupValue::Single)
    };

    SeedFrontMatter {
        time: Some(args.time.clone().unwrap_or_else(|| "1h".into())),
        env: cli_environment.map(|s| s.to_string()),
        app: defaults.app.clone(),
        log_group,
    }
}

fn run_result_path(cwd: &std::path::Path) -> PathBuf {
    let stamp = run_timestamp(Utc::now());
    paths::results_dir(cwd).join(format!("run-{stamp}.jsonl"))
}
