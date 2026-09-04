use std::collections::BTreeMap;

use anyhow::Result;
use chrono::Local;

use crate::cli::CopyLinkArgs;
use crate::commands::fail;
use crate::config::{
    EnvConfig, is_valid_environment_name, load_settings, resolve_env_config, resolve_repo_defaults,
};
use crate::console_link::{LogAnalyticsLinkInput, TimeSpec, build_log_analytics_link, new_tab_id};
use crate::pasteboard::{PasteboardError, copy_to_pasteboard};
use crate::paths;
use crate::query_file::{FrontMatter, LogGroupValue, load_query_file, parse_query_file};
use crate::template::expand_template;
use crate::terminal::{current_platform, default_link_style, open_url_command, styled_link};
use crate::time_range::{
    TimeRange, TimeRangeParseError, parse_time_range, try_parse_relative_duration_seconds,
};

pub async fn run(args: CopyLinkArgs) -> Result<()> {
    if args.query.is_some() && args.query_file.is_some() {
        return Err(fail(2, "--query and --query-file are mutually exclusive"));
    }

    let cwd = std::env::current_dir()?;
    let settings_path = paths::config_path();
    let settings = load_settings(&settings_path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let (section_name, defaults) = resolve_repo_defaults(&settings, &cwd);

    let cli_environment = match &args.environment {
        Some(v) if !is_valid_environment_name(v) => {
            return Err(fail(
                2,
                format!(
                    "--environment must be lower kebab-case (e.g. \"prod\", \"us-east-1\") — got {v:?}"
                ),
            ));
        }
        Some(v) => Some(v.clone()),
        None => None,
    };

    let (query_body, front_matter) = resolve_query_source_read_only(&args).await?;

    let environment = match (cli_environment, &front_matter.env) {
        (Some(e), _) => Some(e),
        (None, Some(e)) => {
            if !is_valid_environment_name(e) {
                return Err(fail(
                    2,
                    format!("front-matter `env` must be lower kebab-case — got {e:?}"),
                ));
            }
            Some(e.clone())
        }
        (None, None) => None,
    };
    let time_expr = args
        .time
        .clone()
        .or_else(|| front_matter.time.clone())
        .unwrap_or_else(|| "1h".into());

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
        &args.log_group,
        front_matter.log_group.as_ref(),
        defaults.group.as_deref(),
        &template_vars,
        section_name.as_deref(),
    )?;

    let expanded_query =
        expand_template(&query_body, &template_vars).map_err(|e| fail(2, e.to_string()))?;

    let env_config = match environment.as_deref() {
        Some(e) => resolve_env_config(&settings, section_name.as_deref(), e),
        None => EnvConfig::default(),
    };
    let region = args
        .region
        .clone()
        .or_else(|| env_config.region.clone())
        .or_else(|| defaults.region.clone())
        .or_else(|| std::env::var("AWS_REGION").ok());

    let region = region.ok_or_else(|| {
        fail(
            2,
            format!(
                "no AWS region — set [env.{}].region in settings.toml, pass --region, or set AWS_REGION",
                environment.as_deref().unwrap_or("<env>")
            ),
        )
    })?;

    // The diagnostic preamble is noise when --raw is set — the caller asked
    // for nothing but the URL.
    let show_diagnostics = !args.quiet && !args.raw;

    let time_spec = choose_time_spec(&time_expr, &range, args.preserve_time_window);

    if show_diagnostics {
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
        eprintln!("  log groups:  {}", log_groups.join(", "));
        eprintln!(
            "  time:        {}",
            match &time_spec {
                TimeSpec::Relative { seconds_back } => format!("RELATIVE -{seconds_back}s"),
                TimeSpec::Absolute { .. } => format!(
                    "ABSOLUTE {}–{}",
                    range.start.to_rfc3339(),
                    range.end.to_rfc3339()
                ),
            }
        );
    }

    let url = build_log_analytics_link(&LogAnalyticsLinkInput {
        region: &region,
        log_groups: &log_groups,
        time: time_spec,
        query: &expanded_query,
        label: app.as_deref(),
        tab_id: &new_tab_id(),
    });

    if args.raw {
        println!("{url}");
        if args.open {
            open_in_browser(&url);
        }
        return Ok(());
    }

    if let Err(err) = copy_to_pasteboard(&url) {
        let PasteboardError(msg) = err;
        return Err(fail(1, msg));
    }

    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        println!(
            "AWS Console link copied to pasteboard. {}",
            styled_link(&url, "Open directly", default_link_style())
        );
    } else {
        println!("AWS Console link copied to pasteboard.");
    }

    if args.open {
        open_in_browser(&url);
    }

    Ok(())
}

fn open_in_browser(url: &str) {
    let cmd = open_url_command(url, current_platform());
    match std::process::Command::new(&cmd.cmd)
        .args(&cmd.args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {}
        Err(e) => eprintln!("error: failed to open browser ({}): {}", cmd.cmd, e),
    }
}

fn choose_time_spec(time_expr: &str, range: &TimeRange, preserve_absolute: bool) -> TimeSpec {
    if !preserve_absolute {
        if let Some(seconds) = try_parse_relative_duration_seconds(time_expr) {
            return TimeSpec::Relative {
                seconds_back: seconds,
            };
        }
    }
    TimeSpec::Absolute {
        start_ms: range.start.timestamp_millis(),
        end_ms: range.end.timestamp_millis(),
    }
}

async fn resolve_query_source_read_only(args: &CopyLinkArgs) -> Result<(String, FrontMatter)> {
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
        if parsed.body.is_empty() {
            return Err(fail(2, format!("{qf} has no query body")));
        }
        return Ok((parsed.body, parsed.front_matter));
    }

    let cwd = std::env::current_dir()?;
    let path = paths::current_insights_path(&cwd);
    if !path.exists() {
        return Err(fail(
            2,
            format!(
                "{} does not exist — run `cloudwatch-insights query` first, or pass --query / --query-file",
                path.display()
            ),
        ));
    }
    let parsed = load_query_file(&path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if parsed.body.is_empty() {
        return Err(fail(2, format!("{} has no query body", path.display())));
    }
    Ok((parsed.body, parsed.front_matter))
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
