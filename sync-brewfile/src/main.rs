use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::{
    cursor::{Hide, MoveUp, Show},
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, read},
    queue,
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find Homebrew packages installed locally that are not listed in the given
/// Brewfile, and offer to add, uninstall, or skip each one.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the Brewfile.
    brewfile: PathBuf,

    /// List the extras and exit, without prompting.
    #[arg(long)]
    list: bool,
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
    run(&args.brewfile, args.list)
}

fn run(brewfile: &Path, list_only: bool) -> Result<()> {
    let listed =
        parse_brewfile(brewfile).with_context(|| format!("reading {}", brewfile.display()))?;

    let installed = installed_top_level()?;

    let listed_keys: BTreeSet<(Kind, &str)> = listed
        .iter()
        .map(|i| (i.kind, short_name(&i.name)))
        .collect();

    let extras: Vec<Item> = installed
        .into_iter()
        .filter(|item| !listed_keys.contains(&(item.kind, short_name(&item.name))))
        .collect();

    if list_only {
        for item in &extras {
            println!("{} \"{}\"", item.kind, item.name);
        }
        return Ok(());
    }

    if extras.is_empty() {
        println!(
            "All top-level installed packages are already listed in {}.",
            brewfile.display()
        );
        return Ok(());
    }

    let mut stdout = io::stdout();
    for item in &extras {
        match prompt_for_item(&mut stdout, brewfile, item)? {
            PromptOutcome::Continue => {}
            PromptOutcome::Quit => break,
        }
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

enum PromptOutcome {
    Continue,
    Quit,
}

const OPTIONS: &[(&str, char)] = &[
    ("Add to Brewfile", 'a'),
    ("Uninstall", 'd'),
    ("Skip", 's'),
    ("Quit", 'q'),
];

fn prompt_for_item<W: Write>(out: &mut W, brewfile: &Path, item: &Item) -> Result<PromptOutcome> {
    let info = fetch_info(item);

    writeln!(out)?;
    let display_name = format!("{} {}", item.kind, item.name);
    match info.as_ref().and_then(|i| i.homepage.as_deref()) {
        Some(url) => writeln!(out, "{}", hyperlink(url, &display_name))?,
        None => writeln!(out, "{display_name}")?,
    }
    if let Some(desc) = info.as_ref().and_then(|i| i.desc.as_deref()) {
        writeln!(out, "{desc}")?;
    }
    writeln!(out)?;

    let choice = run_menu(out)?;

    if let Some(idx) = choice {
        queue!(out, SetForegroundColor(Color::Cyan))?;
        write!(out, "→ ")?;
        queue!(out, ResetColor)?;
        writeln!(out, "{}", OPTIONS[idx].0)?;
    }

    match choice {
        Some(0) => {
            let comment = read_optional_comment(out)?;
            add_to_brewfile(brewfile, item, comment.as_deref())?;
        }
        Some(1) => {
            if let Err(err) = uninstall(item) {
                writeln!(out, "uninstall failed: {err:#}")?;
            }
        }
        Some(2) => {}
        Some(3) | None => return Ok(PromptOutcome::Quit),
        _ => unreachable!(),
    }
    Ok(PromptOutcome::Continue)
}

fn read_optional_comment<W: Write>(out: &mut W) -> Result<Option<String>> {
    write!(out, "Comment (optional): ")?;
    out.flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        writeln!(out)?;
        return Ok(None);
    }
    let trimmed = line.trim_matches(['\r', '\n', ' ', '\t']);
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// Renders a 3-option list and reads keystrokes until the user makes a choice.
/// Returns `None` if the user pressed q / Esc / Ctrl+C.
fn run_menu<W: Write>(out: &mut W) -> Result<Option<usize>> {
    let n = OPTIONS.len();
    let mut sel = 0usize;

    enable_raw_mode()?;
    queue!(out, Hide)?;
    out.flush()?;
    let raw_guard = scopeguard(|| {
        let _ = disable_raw_mode();
        let _ = queue!(io::stdout(), Show);
        let _ = io::stdout().flush();
    });

    render_menu(out, sel)?;

    let result = loop {
        match read()? {
            Event::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..
            }) => continue,
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                let mut chosen: Option<usize> = None;
                match code {
                    KeyCode::Char('j') | KeyCode::Down => sel = (sel + 1) % n,
                    KeyCode::Char('k') | KeyCode::Up => sel = (sel + n - 1) % n,
                    KeyCode::Enter => chosen = Some(sel),
                    KeyCode::Char('c' | 'C') if modifiers.contains(KeyModifiers::CONTROL) => {
                        break Ok(None);
                    }
                    KeyCode::Esc | KeyCode::Char('q' | 'Q') => break Ok(None),
                    KeyCode::Char(c) => {
                        if let Some(i) = OPTIONS
                            .iter()
                            .position(|(_, key)| c.eq_ignore_ascii_case(key))
                        {
                            chosen = Some(i);
                        }
                    }
                    _ => {}
                }
                if let Some(i) = chosen {
                    break Ok(Some(i));
                }
                queue!(out, MoveUp(n as u16), Clear(ClearType::FromCursorDown))?;
                render_menu(out, sel)?;
            }
            _ => {}
        }
    };

    // Clear the menu so any per-action output starts on a clean line.
    queue!(out, MoveUp(n as u16), Clear(ClearType::FromCursorDown))?;
    out.flush()?;
    drop(raw_guard);
    result
}

fn render_menu<W: Write>(out: &mut W, sel: usize) -> Result<()> {
    for (i, (label, key)) in OPTIONS.iter().enumerate() {
        // \r needed because raw mode disables NL→CRNL translation.
        if i == sel {
            queue!(
                out,
                SetForegroundColor(Color::Cyan),
                SetAttribute(Attribute::Bold),
            )?;
            write!(out, "  ❯ {label:<16}")?;
            queue!(out, SetAttribute(Attribute::Reset))?;
            queue!(out, SetForegroundColor(Color::Cyan))?;
            write!(out, " {key}")?;
            queue!(out, ResetColor)?;
        } else {
            write!(out, "    {label:<16}")?;
            queue!(out, SetAttribute(Attribute::Dim))?;
            write!(out, " {key}")?;
            queue!(out, SetAttribute(Attribute::Reset))?;
        }
        write!(out, "\r\n")?;
    }
    out.flush()?;
    Ok(())
}

/// OSC 8 terminal hyperlink with explicit underline + cyan styling so it
/// looks like a link. Modern terminals (ghostty, iTerm2, kitty, wezterm,
/// recent Apple Terminal) make `text` clickable; the styling makes it
/// visually obvious that it is one.
fn hyperlink(url: &str, text: &str) -> String {
    // SGR: 4 = underline on, 36 = cyan; 39 = default fg, 24 = underline off.
    format!("\x1b]8;;{url}\x1b\\\x1b[4;36m{text}\x1b[24;39m\x1b]8;;\x1b\\")
}

#[derive(Deserialize)]
struct InfoEntry {
    desc: Option<String>,
    homepage: Option<String>,
}

#[derive(Deserialize, Default)]
struct InfoResponse {
    #[serde(default)]
    formulae: Vec<InfoEntry>,
    #[serde(default)]
    casks: Vec<InfoEntry>,
}

fn fetch_info(item: &Item) -> Option<InfoEntry> {
    let mut cmd = Command::new("brew");
    cmd.arg("info").arg("--json=v2");
    if item.kind == Kind::Cask {
        cmd.arg("--cask");
    }
    cmd.arg(&item.name);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let resp: InfoResponse = serde_json::from_slice(&out.stdout).ok()?;
    match item.kind {
        Kind::Brew => resp.formulae.into_iter().next(),
        Kind::Cask => resp.casks.into_iter().next(),
    }
}

/// Tiny RAII guard so we can run cleanup (disabling raw mode) on any exit
/// path, including `?` propagation. Avoids pulling in the `scopeguard` crate.
struct ScopeGuard<F: FnOnce()> {
    f: Option<F>,
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.f.take() {
            f();
        }
    }
}

fn scopeguard<F: FnOnce()>(f: F) -> ScopeGuard<F> {
    ScopeGuard { f: Some(f) }
}

fn add_to_brewfile(brewfile: &Path, item: &Item, comment: Option<&str>) -> Result<()> {
    let new_line = format_entry_line(item, comment);
    let content = fs::read_to_string(brewfile)?;
    let new_content = insert_into_brewfile(&content, item.kind, &new_line);
    fs::write(brewfile, &new_content)?;
    println!("Added `{new_line}` to {}", brewfile.display());
    Ok(())
}

/// Format an entry line, optionally aligning a trailing `# comment` to the
/// 32-character column convention used in many Brewfiles.
fn format_entry_line(item: &Item, comment: Option<&str>) -> String {
    let base = format!("{} \"{}\"", item.kind, item.name);
    match comment {
        Some(c) => {
            const COMMENT_COL: usize = 32;
            let pad = if base.len() < COMMENT_COL {
                " ".repeat(COMMENT_COL - base.len())
            } else {
                "  ".to_string()
            };
            format!("{base}{pad}# {c}")
        }
        None => base,
    }
}

/// Returns a new Brewfile with `new_line` inserted in alphabetical order
/// among existing entries of `kind`. If no entries of that kind exist, the
/// new line is appended at the end of the file.
fn insert_into_brewfile(content: &str, kind: Kind, new_line: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let new_name = parse_quoted_keyword(new_line.trim_start(), kind.keyword())
        .expect("new_line must be a valid `<kind> \"<name>\"` line");

    let same_kind: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            parse_quoted_keyword(line.trim_start(), kind.keyword()).map(|name| (i, name))
        })
        .collect();

    let insert_at = if same_kind.is_empty() {
        lines.len()
    } else {
        match same_kind
            .iter()
            .rposition(|(_, name)| name.as_str() <= new_name.as_str())
        {
            Some(idx) => same_kind[idx].0 + 1,
            None => same_kind[0].0,
        }
    };

    lines.insert(insert_at, new_line.to_string());
    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
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
        let names: Vec<(Kind, &str)> = items.iter().map(|i| (i.kind, i.name.as_str())).collect();

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

    fn brew(name: &str) -> Item {
        Item {
            kind: Kind::Brew,
            name: name.into(),
        }
    }
    fn cask(name: &str) -> Item {
        Item {
            kind: Kind::Cask,
            name: name.into(),
        }
    }
    fn insert(content: &str, item: &Item) -> String {
        let line = format_entry_line(item, None);
        insert_into_brewfile(content, item.kind, &line)
    }

    #[test]
    fn inserts_brew_in_alphabetical_order() {
        let original = "brew \"ansible\"\nbrew \"bat\"\nbrew \"jq\"\n";
        let updated = insert(original, &brew("aria2"));
        assert_eq!(
            updated,
            "brew \"ansible\"\nbrew \"aria2\"\nbrew \"bat\"\nbrew \"jq\"\n"
        );
    }

    #[test]
    fn inserts_brew_before_first_when_smaller_than_all() {
        let original = "brew \"ansible\"\nbrew \"bat\"\n";
        let updated = insert(original, &brew("abc"));
        assert_eq!(updated, "brew \"abc\"\nbrew \"ansible\"\nbrew \"bat\"\n");
    }

    #[test]
    fn inserts_brew_after_last_when_larger_than_all() {
        let original = "brew \"ansible\"\nbrew \"bat\"\n";
        let updated = insert(original, &brew("zoxide"));
        assert_eq!(updated, "brew \"ansible\"\nbrew \"bat\"\nbrew \"zoxide\"\n");
    }

    #[test]
    fn brew_inserts_only_among_brews_not_casks() {
        let original =
            "tap \"foo/bar\"\n\nbrew \"ansible\"\nbrew \"jq\"\n\ncask \"iterm2\"\ncask \"slack\"\n";
        let updated = insert(original, &brew("git"));
        assert_eq!(
            updated,
            "tap \"foo/bar\"\n\nbrew \"ansible\"\nbrew \"git\"\nbrew \"jq\"\n\ncask \"iterm2\"\ncask \"slack\"\n"
        );
    }

    #[test]
    fn cask_inserts_only_among_casks() {
        let original = "brew \"ansible\"\nbrew \"jq\"\n\ncask \"iterm2\"\ncask \"slack\"\n";
        let updated = insert(original, &cask("orbstack"));
        assert_eq!(
            updated,
            "brew \"ansible\"\nbrew \"jq\"\n\ncask \"iterm2\"\ncask \"orbstack\"\ncask \"slack\"\n"
        );
    }

    #[test]
    fn appends_when_no_entries_of_same_kind() {
        let original = "brew \"jq\"\n";
        let updated = insert(original, &cask("iterm2"));
        assert_eq!(updated, "brew \"jq\"\ncask \"iterm2\"\n");
    }

    #[test]
    fn preserves_no_trailing_newline_when_input_has_none() {
        let original = "brew \"jq\"";
        let updated = insert(original, &brew("fd"));
        assert_eq!(updated, "brew \"fd\"\nbrew \"jq\"");
    }

    #[test]
    fn skips_inline_comments_when_finding_kind_lines() {
        let original = "brew \"aria2\"                    # Used by xcodes\nbrew \"bat\"\n";
        let updated = insert(original, &brew("awscli"));
        assert_eq!(
            updated,
            "brew \"aria2\"                    # Used by xcodes\nbrew \"awscli\"\nbrew \"bat\"\n"
        );
    }

    #[test]
    fn formats_entry_with_aligned_comment() {
        let line = format_entry_line(&brew("fnm"), Some("Fast node manager"));
        assert_eq!(
            line,
            "brew \"fnm\"                      # Fast node manager"
        );
    }

    #[test]
    fn formats_entry_without_comment_when_none() {
        assert_eq!(format_entry_line(&brew("jq"), None), "brew \"jq\"");
    }

    #[test]
    fn formats_entry_with_two_space_gap_when_too_long_to_align() {
        let line = format_entry_line(
            &brew("a-very-long-formula-name-that-overflows"),
            Some("note"),
        );
        assert_eq!(
            line,
            "brew \"a-very-long-formula-name-that-overflows\"  # note"
        );
    }

    #[test]
    fn insert_carries_comment_through_to_file() {
        let original = "brew \"ansible\"\nbrew \"bat\"\n";
        let line = format_entry_line(&brew("aria2"), Some("for xcodes"));
        let updated = insert_into_brewfile(original, Kind::Brew, &line);
        assert_eq!(
            updated,
            "brew \"ansible\"\nbrew \"aria2\"                    # for xcodes\nbrew \"bat\"\n"
        );
    }
}
