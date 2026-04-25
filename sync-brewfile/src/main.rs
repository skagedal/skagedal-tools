use anyhow::{Context, Result, bail};
use clap::Parser;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find Homebrew packages installed locally that are not listed in the given
/// Brewfile, and offer to add, uninstall, or skip each one.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the Brewfile.
    brewfile: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
enum Kind {
    Brew,
    Cask,
}

impl Kind {
    fn keyword(self) -> &'static str {
        match self {
            Kind::Brew => "brew",
            Kind::Cask => "cask",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.keyword())
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Item {
    kind: Kind,
    name: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    run(&args.brewfile)
}

fn run(brewfile: &Path) -> Result<()> {
    let listed = parse_brewfile(brewfile)
        .with_context(|| format!("reading {}", brewfile.display()))?;

    let installed = installed_top_level()?;

    let listed_keys: BTreeSet<(Kind, &str)> = listed
        .iter()
        .map(|i| (i.kind, short_name(&i.name)))
        .collect();

    let extras: Vec<Item> = installed
        .into_iter()
        .filter(|item| !listed_keys.contains(&(item.kind, short_name(&item.name))))
        .collect();

    if extras.is_empty() {
        println!(
            "All top-level installed packages are already listed in {}.",
            brewfile.display()
        );
        return Ok(());
    }

    println!(
        "Found {} package(s) not in {}:",
        extras.len(),
        brewfile.display()
    );
    for item in &extras {
        println!("  {} \"{}\"", item.kind, item.name);
    }

    let stdin = io::stdin();
    let mut input = stdin.lock();
    for item in &extras {
        prompt_for_item(brewfile, item, &mut input)?;
    }

    Ok(())
}

/// Parse a Brewfile and return the set of declared `brew` and `cask` entries.
///
/// The format is permissive — we just match `^brew "name"` / `^cask "name"`,
/// ignoring any trailing options or comments. Lines that are blank or start
/// with `#` are skipped.
fn parse_brewfile(path: &Path) -> Result<BTreeSet<Item>> {
    let content = fs::read_to_string(path)?;
    let mut items = BTreeSet::new();
    for line in content.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = parse_quoted_keyword(line, "brew") {
            items.insert(Item {
                kind: Kind::Brew,
                name,
            });
        } else if let Some(name) = parse_quoted_keyword(line, "cask") {
            items.insert(Item {
                kind: Kind::Cask,
                name,
            });
        }
    }
    Ok(items)
}

/// Strip a `user/tap/` prefix from a Homebrew package identifier, returning the
/// "short" name. `brew leaves` reports tap-installed formulae with their full
/// `user/tap/name` prefix, while Brewfiles typically list them by short name.
fn short_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// If `line` starts with `<keyword> "<name>"`, return the name.
fn parse_quoted_keyword(line: &str, keyword: &str) -> Option<String> {
    let rest = line.strip_prefix(keyword)?;
    let rest = rest.strip_prefix(|c: char| c.is_ascii_whitespace())?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Top-level installed packages: formulae installed on request (not pulled in
/// only as a dependency), and all installed casks.
fn installed_top_level() -> Result<BTreeSet<Item>> {
    let mut items = BTreeSet::new();
    for name in run_lines(&["leaves", "--installed-on-request"])? {
        items.insert(Item {
            kind: Kind::Brew,
            name,
        });
    }
    for name in run_lines(&["list", "--cask"])? {
        items.insert(Item {
            kind: Kind::Cask,
            name,
        });
    }
    Ok(items)
}

fn run_lines(args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("brew")
        .args(args)
        .output()
        .with_context(|| format!("running `brew {}`", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "`brew {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn prompt_for_item<R: BufRead>(brewfile: &Path, item: &Item, input: &mut R) -> Result<()> {
    loop {
        println!();
        println!("{} \"{}\"", item.kind, item.name);
        println!("  1) Add to Brewfile");
        println!("  2) Uninstall");
        println!("  3) Keep evaluating (skip)");
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            println!();
            return Ok(());
        }
        match line.trim() {
            "1" => {
                add_to_brewfile(brewfile, item)?;
                return Ok(());
            }
            "2" => {
                if let Err(err) = uninstall(item) {
                    eprintln!("uninstall failed: {err:#}");
                }
                return Ok(());
            }
            "3" | "" => return Ok(()),
            other => eprintln!("Please enter 1, 2, or 3 (got {other:?})."),
        }
    }
}

fn add_to_brewfile(brewfile: &Path, item: &Item) -> Result<()> {
    let mut content = fs::read_to_string(brewfile)?;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{} \"{}\"\n", item.kind, item.name));
    fs::write(brewfile, content)?;
    println!(
        "Added `{} \"{}\"` to {}",
        item.kind,
        item.name,
        brewfile.display()
    );
    Ok(())
}

fn uninstall(item: &Item) -> Result<()> {
    let mut cmd = Command::new("brew");
    cmd.arg("uninstall");
    if item.kind == Kind::Cask {
        cmd.arg("--cask");
    }
    cmd.arg(&item.name);
    let status = cmd
        .status()
        .with_context(|| format!("running `brew uninstall {}`", item.name))?;
    if !status.success() {
        bail!("`brew uninstall {}` exited with {}", item.name, status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(content: &str) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        fs::write(f.path(), content).unwrap();
        f
    }

    #[test]
    fn parses_brews_and_casks_and_skips_others() {
        let f = write_temp(
            r#"
# Comment
tap "homebrew/bundle"
brew "jq"
brew "git", restart_service: :changed   # trailing
cask "iterm2"
mas "Xcode", id: 497799835
"#,
        );

        let items = parse_brewfile(f.path()).unwrap();
        let names: Vec<(Kind, &str)> = items
            .iter()
            .map(|i| (i.kind, i.name.as_str()))
            .collect();

        assert_eq!(
            names,
            vec![
                (Kind::Brew, "git"),
                (Kind::Brew, "jq"),
                (Kind::Cask, "iterm2"),
            ]
        );
    }

    #[test]
    fn short_name_strips_tap_prefix() {
        assert_eq!(short_name("oven-sh/bun/bun"), "bun");
        assert_eq!(short_name("bun"), "bun");
        assert_eq!(short_name(""), "");
    }

    #[test]
    fn appends_with_trailing_newline_when_missing() {
        let f = write_temp("brew \"jq\"");
        add_to_brewfile(
            f.path(),
            &Item {
                kind: Kind::Cask,
                name: "iterm2".into(),
            },
        )
        .unwrap();
        let content = fs::read_to_string(f.path()).unwrap();
        assert_eq!(content, "brew \"jq\"\ncask \"iterm2\"\n");
    }

    #[test]
    fn appends_without_double_newline() {
        let f = write_temp("brew \"jq\"\n");
        add_to_brewfile(
            f.path(),
            &Item {
                kind: Kind::Brew,
                name: "fd".into(),
            },
        )
        .unwrap();
        let content = fs::read_to_string(f.path()).unwrap();
        assert_eq!(content, "brew \"jq\"\nbrew \"fd\"\n");
    }
}
