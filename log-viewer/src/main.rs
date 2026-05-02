mod app;
mod colors;
mod config;
mod entry;
mod source;
mod triggers;
mod ui;

#[cfg(feature = "web")]
mod web;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::app::App;
use crate::config::{Config, config_path, ensure_config_file, load_with_profile};
use crate::source::SourceSpec;
use crate::triggers::TriggerRuntime;

const DESCRIPTION: &str = "View JSONL logs in a TUI or webview-embedded React app.";

#[derive(Parser, Debug)]
#[command(name = "log-viewer", about = DESCRIPTION, version)]
struct Cli {
    /// JSONL file to read; use "-" for stdin.
    #[arg(short = 'f', long = "file")]
    file: Option<String>,

    /// Path to a config file.
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,

    /// Activate a profile defined in the config file.
    #[arg(long = "profile")]
    profile: Option<String>,

    /// Open the embedded React app in a webview (browser-style GUI).
    #[cfg_attr(
        feature = "web",
        arg(short = 'w', long = "web", default_value_t = false)
    )]
    #[cfg_attr(
        not(feature = "web"),
        arg(short = 'w', long = "web", default_value_t = false, hide = true)
    )]
    web: bool,

    /// Positional file path (alternative to -f).
    path: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<SubCmd>,
}

#[derive(Subcommand, Debug)]
enum SubCmd {
    /// Create the config file if missing, then open it in $EDITOR.
    EditConfig {
        #[arg(short = 'c', long = "config")]
        config: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let raw_argv: Vec<OsString> = std::env::args_os().collect();
    let (rest, exec_argv) = extract_exec(&raw_argv);
    match Cli::try_parse_from(rest) {
        Ok(cli) => match dispatch(cli, exec_argv) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err:#}");
                ExitCode::from(1)
            }
        },
        Err(err) => {
            // clap prints help/version itself and uses exit code 0 for those.
            err.print().ok();
            if err.use_stderr() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

fn dispatch(cli: Cli, exec_argv: Option<Vec<OsString>>) -> Result<()> {
    if let Some(SubCmd::EditConfig { config }) = cli.command {
        let path = config.unwrap_or_else(config_path);
        let created = ensure_config_file(&path)?;
        if created {
            eprintln!("Created {}", path.display());
        }
        return open_editor(&path);
    }

    let cfg_path = cli.config.clone().unwrap_or_else(config_path);
    if cli.config.is_some() && !cfg_path.exists() {
        anyhow::bail!("config file not found: {}", cfg_path.display());
    }
    let config = load_with_profile(&cfg_path, cli.profile.as_deref())?;
    let spec = resolve_source(&cli, exec_argv)?;
    run_with(spec, config, cli.web)
}

fn run_with(spec: SourceSpec, config: Config, want_web: bool) -> Result<()> {
    let label = spec.label();
    let stream = source::start(&spec, &config.default_field, &config.flatten_fields)
        .with_context(|| format!("starting source {label}"))?;
    let triggers = TriggerRuntime::new(config.triggers.clone());

    if want_web {
        #[cfg(feature = "web")]
        {
            return web::run(config, label, stream, triggers);
        }
        #[cfg(not(feature = "web"))]
        {
            let _ = (config, label, stream, triggers);
            anyhow::bail!(
                "this build was compiled without the `web` feature; rebuild with --features web"
            );
        }
    }

    let mut app = App::new(&config, label);
    ui::run(&mut app, stream, triggers)
}

fn resolve_source(cli: &Cli, exec_argv: Option<Vec<OsString>>) -> Result<SourceSpec> {
    if let Some(argv) = exec_argv {
        if argv.is_empty() {
            anyhow::bail!("--exec requires a command");
        }
        if cli.file.is_some() || cli.path.is_some() {
            anyhow::bail!("--file/positional path and --exec are mutually exclusive");
        }
        return Ok(SourceSpec::Command(argv));
    }
    if let Some(file) = &cli.file {
        if file == "-" {
            return Ok(SourceSpec::Stdin);
        }
        let path = PathBuf::from(file);
        if !path.exists() {
            anyhow::bail!("file not found: {}", path.display());
        }
        return Ok(SourceSpec::File(path));
    }
    if let Some(path) = &cli.path {
        if !path.exists() {
            anyhow::bail!("file not found: {}", path.display());
        }
        return Ok(SourceSpec::File(path.clone()));
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Ok(SourceSpec::Stdin);
    }
    anyhow::bail!(
        "no input given — pass a JSONL file (or '-' for stdin), use --exec, or pipe data in"
    );
}

/// Pull `--exec`/`-e` and the rest of argv that follows it. Everything from
/// the flag onward (after its value) is consumed as positional args to the
/// executed command, mirroring the TS log-viewer's behavior.
fn extract_exec(argv: &[OsString]) -> (Vec<OsString>, Option<Vec<OsString>>) {
    let mut rest = Vec::new();
    for arg in argv {
        let s = arg.to_string_lossy();
        if s == "--exec" || s == "-e" {
            let exec: Vec<OsString> = argv.iter().skip(rest.len() + 1).cloned().collect();
            return (rest, Some(exec));
        }
        if let Some(value) = s.strip_prefix("--exec=") {
            let mut exec = vec![OsString::from(value)];
            exec.extend(argv.iter().skip(rest.len() + 1).cloned());
            return (rest, Some(exec));
        }
        rest.push(arg.clone());
    }
    (rest, None)
}

fn open_editor(path: &std::path::Path) -> Result<()> {
    let editor = std::env::var_os("VISUAL")
        .or_else(|| std::env::var_os("EDITOR"))
        .ok_or_else(|| anyhow::anyhow!("no editor set — define $EDITOR (or $VISUAL) and retry"))?;
    let parts = shell_split(&editor.to_string_lossy());
    let (cmd, args) = parts
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("empty editor"))?;
    let status = Command::new(cmd)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("launching editor {cmd}"))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn shell_split(s: &str) -> Vec<String> {
    s.split_whitespace().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osv(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(|s| OsString::from(*s)).collect()
    }

    #[test]
    fn extract_exec_with_long_flag() {
        let argv = osv(&["bin", "--profile", "stern", "--exec", "kubectl", "logs", "-f", "pod"]);
        let (rest, exec) = extract_exec(&argv);
        assert_eq!(
            rest.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["bin", "--profile", "stern"]
        );
        let exec = exec.unwrap();
        assert_eq!(
            exec.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["kubectl", "logs", "-f", "pod"]
        );
    }

    #[test]
    fn extract_exec_with_short_flag() {
        let argv = osv(&["bin", "-e", "stern", "--output", "json", "app"]);
        let (rest, exec) = extract_exec(&argv);
        assert_eq!(
            rest.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["bin"]
        );
        assert_eq!(
            exec.unwrap()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["stern", "--output", "json", "app"]
        );
    }

    #[test]
    fn extract_exec_with_equals_form() {
        let argv = osv(&["bin", "--exec=foo", "bar", "baz"]);
        let (_, exec) = extract_exec(&argv);
        assert_eq!(
            exec.unwrap()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["foo", "bar", "baz"]
        );
    }

    #[test]
    fn extract_exec_returns_none_when_absent() {
        let argv = osv(&["bin", "--profile", "stern", "file.jsonl"]);
        let (_, exec) = extract_exec(&argv);
        assert!(exec.is_none());
    }
}
