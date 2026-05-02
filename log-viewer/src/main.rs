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

    /// Run a command and view its stdout as logs. Everything after this flag
    /// is the command to run, e.g. `--exec kubectl logs -f pod`.
    #[arg(
        short = 'e',
        long = "exec",
        num_args = 1..,
        value_name = "COMMAND",
        allow_hyphen_values = true,
        conflicts_with_all = ["file", "path"],
    )]
    exec: Option<Vec<OsString>>,

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
    match Cli::try_parse() {
        Ok(cli) => match dispatch(cli) {
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

fn dispatch(cli: Cli) -> Result<()> {
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
    let spec = resolve_source(&cli)?;
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
    // Live sources (--exec) start in follow mode by default — the user
    // almost always wants to see new entries as they arrive.
    if matches!(spec, SourceSpec::Command(_)) {
        app.follow = true;
    }
    ui::run(&mut app, stream, triggers)
}

fn resolve_source(cli: &Cli) -> Result<SourceSpec> {
    if let Some(argv) = &cli.exec {
        return Ok(SourceSpec::Command(argv.clone()));
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
    use clap::Parser;

    fn exec_strings(cli: &Cli) -> Vec<String> {
        cli.exec
            .as_ref()
            .expect("expected --exec to be present")
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn parses_exec_with_long_flag() {
        let cli = Cli::try_parse_from([
            "bin", "--profile", "stern", "--exec", "kubectl", "logs", "-f", "pod",
        ])
        .unwrap();
        assert_eq!(cli.profile.as_deref(), Some("stern"));
        assert_eq!(exec_strings(&cli), vec!["kubectl", "logs", "-f", "pod"]);
    }

    #[test]
    fn parses_exec_with_short_flag() {
        let cli =
            Cli::try_parse_from(["bin", "-e", "stern", "--output", "json", "app"]).unwrap();
        assert_eq!(exec_strings(&cli), vec!["stern", "--output", "json", "app"]);
    }

    #[test]
    fn parses_exec_with_equals_form_single_value() {
        // The `=` form provides exactly one value to the flag. To pass a
        // multi-word command, use the spaced form: `--exec foo bar baz`.
        let cli = Cli::try_parse_from(["bin", "--exec=foo"]).unwrap();
        assert_eq!(exec_strings(&cli), vec!["foo"]);
    }

    #[test]
    fn exec_absent_when_not_passed() {
        let cli = Cli::try_parse_from(["bin", "--profile", "stern", "file.jsonl"]).unwrap();
        assert!(cli.exec.is_none());
    }

    #[test]
    fn exec_conflicts_with_positional_path() {
        let err = Cli::try_parse_from(["bin", "file.jsonl", "--exec", "kubectl"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn exec_requires_a_command() {
        let err = Cli::try_parse_from(["bin", "--exec"]).unwrap_err();
        // Either "missing required value" or similar — point is, it errors.
        assert!(matches!(
            err.kind(),
            clap::error::ErrorKind::InvalidValue
                | clap::error::ErrorKind::MissingRequiredArgument
                | clap::error::ErrorKind::WrongNumberOfValues
        ));
    }
}
