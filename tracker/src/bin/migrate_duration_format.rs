//! Migration tool to rewrite durations in week files in the compound format.
//!
//! Week files written before the duration format was unified spell durations
//! out as separate hour and minute terms, `* balance 3h 12m`. Those still
//! parse, but tracker now writes `* balance 3:12h`, and this tool brings old
//! files over so the whole archive reads the same way.
//!
//! Usage:
//!   cargo run --bin migrate_duration_format [--dry-run] [--fix-negative-minutes]
//!
//! Options:
//!   --dry-run                Show what would change without writing anything
//!   --fix-negative-minutes   Also rewrite ambiguous lines, see below
//!
//! A line like `* balance -9h 26m` is ambiguous. Read term by term, which is
//! how tracker has always read it, it means -9h + 26m = -8:34h. But it is also
//! exactly what the old report printed for -9:26h, and copying that output into
//! a week file was the way a balance got carried over — so it most likely means
//! -9:26h. The tool leaves such lines alone and lists them, unless
//! `--fix-negative-minutes` is given, in which case they are read as -9:26h.
//! Lines where the minutes are zero, `* balance -8h 0m`, are not ambiguous.
//!
//! Running this more than once is harmless: durations already in the compound
//! format are left untouched.

use regex::Regex;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tracker::duration::{format_duration, parse_duration};
use tracker::paths::TrackerDirs;

/// `[year]-W[week].txt`, the name of a week file. Anything else in the
/// directory — editor autosaves, notes — is left alone.
static WEEK_FILE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{4}-W[0-9]{2}\.txt$").unwrap());

/// A shift line carrying a duration rather than a time span: `* balance 3h 12m`.
static DURATION_LINE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\* (?P<text>[A-Za-z]+)\s+(?P<duration>\S.*?)\s*$").unwrap());

/// Negative hours followed by unsigned, non-zero minutes: `-9h 26m`.
static AMBIGUOUS_DURATION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-\s*(?P<hours>[0-9]+)\s*h\s+(?P<minutes>[0-9]*[1-9][0-9]*)\s*m$").unwrap()
});

/// One line that would change: (line number, old line, new line).
type Rewrite = (usize, String, String);

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let fix_negative_minutes = args.iter().any(|arg| arg == "--fix-negative-minutes");
    if let Some(unknown) = args
        .iter()
        .find(|arg| !matches!(arg.as_str(), "--dry-run" | "--fix-negative-minutes"))
    {
        eprintln!("Unknown option: {}", unknown);
        eprintln!("Usage: migrate_duration_format [--dry-run] [--fix-negative-minutes]");
        std::process::exit(1);
    }

    let dirs = TrackerDirs::real();
    let week_files_dir = dirs.data_dir().join("week-files");

    if !week_files_dir.exists() {
        eprintln!(
            "Week files directory does not exist: {}",
            week_files_dir.display()
        );
        eprintln!("Nothing to migrate.");
        return;
    }

    println!("Scanning directory: {}", week_files_dir.display());
    println!(
        "Mode: {}",
        if dry_run {
            "DRY RUN (no changes will be made)"
        } else {
            "LIVE (files will be rewritten)"
        }
    );
    println!();

    let mut paths = match week_files(&week_files_dir) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("Error reading directory: {}", err);
            std::process::exit(1);
        }
    };
    paths.sort();

    let mut changed_files = 0;
    let mut changed_lines = 0;
    let mut ambiguous: Vec<(String, usize, String)> = Vec::new();

    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                eprintln!("{}: could not read: {}", name, err);
                continue;
            }
        };

        for (line_num, line) in content.lines().enumerate() {
            if let Some(duration) = duration_text(line)
                && AMBIGUOUS_DURATION_REGEX.is_match(duration)
            {
                ambiguous.push((name.clone(), line_num + 1, line.to_string()));
            }
        }

        let (migrated, rewrites) = migrate_content(&content, fix_negative_minutes);
        if rewrites.is_empty() {
            continue;
        }

        changed_files += 1;
        changed_lines += rewrites.len();
        println!("{}", name);
        for (line_num, old, new) in &rewrites {
            println!("  line {}: {}  ->  {}", line_num, old, new);
        }

        if !dry_run && let Err(err) = fs::write(&path, &migrated) {
            eprintln!("  ERROR: could not write: {}", err);
        }
    }

    println!();
    if changed_files == 0 {
        println!("No durations need migration.");
    } else if dry_run {
        println!(
            "Would rewrite {} duration(s) in {} file(s).",
            changed_lines, changed_files
        );
    } else {
        println!(
            "Rewrote {} duration(s) in {} file(s).",
            changed_lines, changed_files
        );
    }

    if !ambiguous.is_empty() && !fix_negative_minutes {
        println!();
        println!(
            "Left {} ambiguous duration(s) alone — negative hours with unsigned",
            ambiguous.len()
        );
        println!("minutes, which read term by term as, for instance, -9h + 26m:");
        for (name, line_num, line) in &ambiguous {
            println!("  {}:{}: {}", name, line_num, line);
        }
        println!();
        println!("Re-run with --fix-negative-minutes to read those as -9:26h instead.");
    }
}

fn week_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let is_week_file = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| WEEK_FILE_REGEX.is_match(name));
        if is_week_file {
            paths.push(path);
        }
    }
    Ok(paths)
}

/// The duration part of a duration shift line, if the line is one.
fn duration_text(line: &str) -> Option<&str> {
    let captures = DURATION_LINE_REGEX.captures(line)?;
    let duration = captures.name("duration").unwrap().as_str();
    // Special shifts (`* VAB 13:00-17:00`) match the same shape, so only lines
    // whose second part really is a duration count.
    parse_duration(duration).ok()?;
    Some(duration)
}

/// Rewrites every duration in the file in the compound format, returning the
/// new content along with the lines that changed.
fn migrate_content(content: &str, fix_negative_minutes: bool) -> (String, Vec<Rewrite>) {
    let mut rewrites = Vec::new();
    let mut migrated = String::with_capacity(content.len());

    for (line_num, line) in content.lines().enumerate() {
        let new_line = migrate_line(line, fix_negative_minutes);
        if new_line != line {
            rewrites.push((line_num + 1, line.to_string(), new_line.clone()));
        }
        migrated.push_str(&new_line);
        migrated.push('\n');
    }

    (migrated, rewrites)
}

fn migrate_line(line: &str, fix_negative_minutes: bool) -> String {
    let Some(captures) = DURATION_LINE_REGEX.captures(line) else {
        return line.to_string();
    };
    let text = captures.name("text").unwrap().as_str();
    let written = captures.name("duration").unwrap().as_str();

    let duration = match AMBIGUOUS_DURATION_REGEX.captures(written) {
        Some(ambiguous) if fix_negative_minutes => {
            let hours: i64 = ambiguous.name("hours").unwrap().as_str().parse().unwrap();
            let minutes: i64 = ambiguous.name("minutes").unwrap().as_str().parse().unwrap();
            match chrono::TimeDelta::try_minutes(-(hours * 60 + minutes)) {
                Some(duration) => duration,
                None => return line.to_string(),
            }
        }
        // Ambiguous and not asked to reinterpret: leave the line as written.
        Some(_) => return line.to_string(),
        None => match parse_duration(written) {
            Ok(duration) => duration,
            // Not a duration at all — a special shift, or something we don't
            // understand. Either way, not ours to touch.
            Err(_) => return line.to_string(),
        },
    };

    format!("* {} {}", text, format_duration(duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_durations_in_the_compound_format() {
        assert_eq!("* balance 3:12h", migrate_line("* balance 3h 12m", false));
        assert_eq!("* balance 20h", migrate_line("* balance 20 h 0 m", false));
        assert_eq!(
            "* balance -4:43h",
            migrate_line("* balance -4h -43m", false)
        );
        assert_eq!("* balance -8h", migrate_line("* balance -8h 0m", false));
        assert_eq!("* carry 9m", migrate_line("* carry 0h 9m", false));
    }

    #[test]
    fn leaves_everything_else_alone() {
        for line in [
            "* 08:32-12:02",
            "* 08:12-",
            "* Vacation",
            "* VAB 13:00-17:00",
            "# This should be -4h -43m but my software is buggy. :( ",
            "[monday 2020-07-13]",
            "",
        ] {
            assert_eq!(line, migrate_line(line, false));
        }
    }

    #[test]
    fn leaves_durations_already_migrated_alone() {
        for line in ["* balance 3:12h", "* balance -4:43h", "* balance -45m"] {
            assert_eq!(line, migrate_line(line, false));
        }
    }

    #[test]
    fn leaves_ambiguous_durations_alone_by_default() {
        assert_eq!(
            "* balance -9h 26m",
            migrate_line("* balance -9h 26m", false)
        );
        // Read term by term this is -8:34h; the flag says to read it as the
        // old report output it was copied from.
        assert_eq!("* balance -9:26h", migrate_line("* balance -9h 26m", true));
    }

    #[test]
    fn keeps_the_rest_of_the_file_byte_for_byte() {
        let content = "* balance 1h 10m\n\n[monday 2020-07-13]\n* Vacation\n# Note\n";
        let (migrated, rewrites) = migrate_content(content, false);
        assert_eq!(
            "* balance 1:10h\n\n[monday 2020-07-13]\n* Vacation\n# Note\n",
            migrated
        );
        assert_eq!(1, rewrites.len());
    }
}
