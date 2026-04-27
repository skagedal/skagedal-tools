use anyhow::{Context, Result, bail};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const DRIVER_NAME: &str = "skagedal-package-json";
const DRIVER_DESCRIPTION: &str = "package.json semver-aware merge";
const DRIVER_COMMAND: &str = "package-json-merge merge %O %A %B %P";
const ATTRIBUTES_LINE: &str = "package.json merge=skagedal-package-json";

pub fn install(global: bool) -> Result<()> {
    set_git_config(&format!("merge.{DRIVER_NAME}.name"), DRIVER_DESCRIPTION, global)?;
    set_git_config(&format!("merge.{DRIVER_NAME}.driver"), DRIVER_COMMAND, global)?;

    let attributes_path = resolve_attributes_path(global)?;
    ensure_line_in_file(&attributes_path, ATTRIBUTES_LINE)?;

    let scope = if global { "globally" } else { "in this repository" };
    eprintln!("Installed merge driver \"{DRIVER_NAME}\" {scope}.");
    eprintln!("  attributes: {}", attributes_path.display());
    Ok(())
}

pub fn uninstall(global: bool) -> Result<()> {
    unset_git_section(&format!("merge.{DRIVER_NAME}"), global);

    let attributes_path = resolve_attributes_path(global)?;
    remove_line_from_file(&attributes_path, ATTRIBUTES_LINE)?;

    let scope = if global { "globally" } else { "from this repository" };
    eprintln!("Removed merge driver \"{DRIVER_NAME}\" {scope}.");
    Ok(())
}

fn set_git_config(key: &str, value: &str, global: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("config");
    if global {
        cmd.arg("--global");
    }
    cmd.arg(key).arg(value);
    let status = cmd.status().context("running git config")?;
    if !status.success() {
        bail!("git config {key} failed");
    }
    Ok(())
}

fn unset_git_section(section: &str, global: bool) {
    let mut cmd = Command::new("git");
    cmd.arg("config");
    if global {
        cmd.arg("--global");
    }
    cmd.arg("--remove-section").arg(section);
    // Allow non-zero exit (section may not exist).
    let _ = cmd.status();
}

fn resolve_attributes_path(global: bool) -> Result<PathBuf> {
    if !global {
        let top = git_top_level()?;
        return Ok(top.join(".git").join("info").join("attributes"));
    }
    if let Some(p) = git_global_attributes_file()? {
        return Ok(expand_home(&p));
    }
    let base = match env::var_os("XDG_CONFIG_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home_dir()?.join(".config"),
    };
    Ok(base.join("git").join("attributes"))
}

fn git_top_level() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("running git rev-parse")?;
    if !output.status.success() {
        bail!("not inside a git repository");
    }
    let s = String::from_utf8(output.stdout).context("git rev-parse output not utf-8")?;
    Ok(PathBuf::from(s.trim()))
}

fn git_global_attributes_file() -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["config", "--global", "core.attributesFile"])
        .output()
        .context("running git config")?;
    if !output.status.success() {
        return Ok(None);
    }
    let s = String::from_utf8(output.stdout).context("git config output not utf-8")?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir().unwrap_or_else(|_| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn ensure_line_in_file(path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        f.write_all(b"\n")?;
    }
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

fn remove_line_from_file(path: &Path, line: &str) -> Result<()> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };
    let filtered: Vec<&str> = content.lines().filter(|l| l.trim() != line).collect();
    let mut out = filtered.join("\n");
    if content.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
