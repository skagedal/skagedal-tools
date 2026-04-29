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
    is_valid_environment_name, load_settings, resolve_env_config, resolve_repo_defaults, RepoConfig,
};
use crate::editor::open_editor;
use crate::flatten::flatten_row;
use crate::insights::{run_insights_query, RunQueryOptions, StatusCallback};
use crate::output::{update_latest_symlink, write_results};
use crate::paths;
use crate::query_file::{
    ensure_current_insights, load_query_file, parse_query_file, reset_current_insights,
    run_timestamp, FrontMatter, LogGroupValue, SeedFrontMatter, SeedOptions,
};
use crate::template::expand_template;
use crate::time_range::{parse_time_range, TimeRangeParseError};

pub async fn run(args: QueryArgs) -> Result<()> {
    if args.query.is_some() && args.query_file.is_some() {
        return Err(fail(2, "--query and --query-file are mutually exclusive"));
    }

    let cwd = std::env::current_dir()?;
    let settings_path = paths::config_path();
    let settings = load_settings(&settings_path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let (section_name, defaults) = resolve_repo_defaults(&settings, &cwd);

    let cli_environment = validate_environment_arg(args.environment.as_deref(), "--environment")?;

    let (query_body, front_matter) = resolve_query_source(&args, &defaults, cli_environment.as_deref()).await?;

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

    let expanded_query = expand_template(&query_body, &template_vars)
        .map_err(|e| fail(2, e.to_string()))?;

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

    if let Some(p) = &profile {
        unsafe {
            std::env::set_var("AWS_PROFILE", p);
        }
    }

    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(r) = &region {
        loader = loader.region(aws_config::Region::new(r.clone()));
    }
    let aws_config = loader.load().await;
    let client = Client::new(&aws_config);

    if !args.quiet {
        if let Some(p) = &profile {
            if args.profile.is_none() {
                if let Some(env) = &environment {
                    eprintln!("  AWS_PROFILE: {p} (from [env.{env}])");
                }
            }
        }
        if args.region.is_none() {
            if let Some(r) = &env_config.region {
                if let Some(env) = &environment {
                    eprintln!("  AWS region:  {r} (from [env.{env}])");
                }
            } else if let Some(r) = &defaults.region {
                let from = if let Some(s) = &section_name {
                    format!("[repo.{s}] or [defaults]")
                } else {
                    "[defaults]".to_string()
                };
                eprintln!("  AWS region:  {r} (from {from})");
            }
        }
    }

    if !args.quiet {
        eprintln!(
            "Querying {n} log group(s) from {start} to {end}",
            n = log_groups.len(),
            start = range.start.to_rfc3339(),
            end = range.end.to_rfc3339(),
        );
        eprintln!("  log groups: {}", log_groups.join(", "));
    }

    let quiet = args.quiet;
    let mut last_status = String::new();
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
        query_string: &expanded_query,
        start_time: range.start,
        end_time: range.end,
        limit,
        poll_interval: Duration::from_secs(1),
        on_status,
    })
    .await?;

    let flatten_fields = defaults.flatten_fields.clone().unwrap_or_default();
    let rows: Vec<_> = result
        .rows
        .iter()
        .map(|r| flatten_row(r, &flatten_fields))
        .collect();

    let out_path = run_result_path(&cwd);
    write_results(&out_path, &rows)?;
    let link_path = update_latest_symlink(&out_path)?;

    println!("{}", out_path.display());

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
        eprintln!("  {} → {}", link_path.display(), out_path.display());
    }
    Ok(())
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
    cli: &[String],
    fm_log_group: Option<&LogGroupValue>,
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

    let cli: Vec<String> = cli.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if !cli.is_empty() {
        return expand(cli);
    }
    if let Some(lg) = fm_log_group {
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
        let seeded = ensure_current_insights(&path, &seed).map_err(|e| anyhow::anyhow!(e.to_string()))?;
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
