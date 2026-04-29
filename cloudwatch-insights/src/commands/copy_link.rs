use std::collections::BTreeMap;

use anyhow::Result;
use aws_config::BehaviorVersion;
use chrono::Local;

use crate::cli::CopyLinkArgs;
use crate::commands::fail;
use crate::config::{
    is_valid_environment_name, load_settings, resolve_env_config, resolve_repo_defaults,
    EnvConfig,
};
use crate::console_link::{
    build_console_link, build_query_detail, log_group_arn, ConsoleLinkInput, QueryDetailInput,
    TimeSpec,
};
use crate::pasteboard::{copy_to_pasteboard, PasteboardError};
use crate::paths;
use crate::query_file::{load_query_file, parse_query_file, FrontMatter, LogGroupValue};
use crate::template::expand_template;
use crate::terminal::{current_platform, default_link_style, open_url_command, styled_link};
use crate::time_range::{
    parse_time_range, try_parse_relative_duration_seconds, TimeRange, TimeRangeParseError,
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
                format!("--environment must be lower kebab-case (e.g. \"prod\", \"us-east-1\") — got {v:?}"),
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

    let expanded_query = expand_template(&query_body, &template_vars)
        .map_err(|e| fail(2, e.to_string()))?;

    let env_config = match environment.as_deref() {
        Some(e) => resolve_env_config(&settings, section_name.as_deref(), e),
        None => EnvConfig::default(),
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

    let account_id = resolve_account_id(
        args.account_id.as_deref(),
        &env_config,
        &region,
        profile.as_deref(),
        !show_diagnostics,
        environment.as_deref(),
    )
    .await?;

    let arns: Vec<String> = log_groups
        .iter()
        .map(|name| log_group_arn(&region, &account_id, name))
        .collect();

    let time_spec = choose_time_spec(&time_expr, &range, args.preserve_time_window);

    if show_diagnostics {
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
        eprintln!("  account ID:  {account_id}");
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

    let detail = build_query_detail(&QueryDetailInput {
        query: &expanded_query,
        log_group_arns: &arns,
        time: time_spec,
        query_id: None,
        log_class: None,
    });
    let url = build_console_link(&ConsoleLinkInput {
        region: &region,
        query_detail: &detail,
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
            return TimeSpec::Relative { seconds_back: seconds };
        }
    }
    TimeSpec::Absolute {
        start_ms: range.start.timestamp_millis(),
        end_ms: range.end.timestamp_millis(),
    }
}

async fn resolve_account_id(
    cli_account_id: Option<&str>,
    env_config: &EnvConfig,
    region: &str,
    profile: Option<&str>,
    quiet: bool,
    environment_label: Option<&str>,
) -> Result<String> {
    if let Some(id) = cli_account_id {
        return Ok(id.to_string());
    }
    if let Some(id) = &env_config.account_id {
        return Ok(id.clone());
    }
    if !quiet {
        eprintln!(
            "  looking up account ID via STS (set [env.{}].account-id to skip)",
            environment_label.unwrap_or("<env>")
        );
    }
    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    loader = loader.region(aws_config::Region::new(region.to_string()));
    if let Some(p) = profile {
        loader = loader.profile_name(p.to_string());
    }
    let config = loader.load().await;
    let sts = aws_sdk_sts::Client::new(&config);
    match sts.get_caller_identity().send().await {
        Ok(resp) => match resp.account {
            Some(a) => Ok(a),
            None => Err(fail(1, "STS GetCallerIdentity returned no Account")),
        },
        Err(e) => Err(fail(
            1,
            format!(
                "could not look up AWS account ID via STS: {e}\n  hint: set [env.{}].account-id in settings.toml, or pass --account-id",
                environment_label.unwrap_or("<env>")
            ),
        )),
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
