use crossterm::style::Stylize;
use semver::Version;
use serde::Deserialize;
use std::io::{self, Write};

#[derive(Debug, Deserialize)]
pub struct DependencyChanges {
    #[serde(default)]
    pub added: Vec<DependencyChange>,
    #[serde(default)]
    pub modified: Vec<DependencyChange>,
    #[serde(default)]
    pub removed: Vec<DependencyChange>,
}

#[derive(Debug, Deserialize)]
pub struct DependencyChange {
    pub name: String,
    pub ecosystem: String,
    pub manifest_path: String,
    pub dependency_type: String,
    #[serde(default)]
    pub from_requirement: Option<String>,
    #[serde(default)]
    pub to_requirement: Option<String>,
}

/// Returns `Some(true)` if the bump from `from` to `to` is a major (i.e. potentially
/// breaking) version change per cargo's caret-compatibility rules — the leading
/// non-zero component changing. Returns `None` if either version can't be parsed.
pub fn is_major_bump(from: &str, to: &str) -> Option<bool> {
    let from_v = parse_lax_version(from)?;
    let to_v = parse_lax_version(to)?;
    Some(is_major(&from_v, &to_v))
}

fn is_major(from: &Version, to: &Version) -> bool {
    if from.major > 0 || to.major > 0 {
        from.major != to.major
    } else if from.minor > 0 || to.minor > 0 {
        from.minor != to.minor
    } else {
        from.patch != to.patch
    }
}

/// Parses a version string, accepting partial forms like `"3"` or `"3.5"` by
/// padding missing components with zeros. Returns `None` for non-numeric strings
/// like `"*"`.
fn parse_lax_version(s: &str) -> Option<Version> {
    if let Ok(v) = Version::parse(s) {
        return Some(v);
    }
    let parts: Vec<&str> = s.splitn(3, '.').collect();
    let padded = match parts.as_slice() {
        [a] => format!("{a}.0.0"),
        [a, b] => format!("{a}.{b}.0"),
        _ => return None,
    };
    Version::parse(&padded).ok()
}

pub fn write_dependency_changes(
    w: &mut dyn Write,
    changes: &DependencyChanges,
    color: bool,
) -> io::Result<()> {
    let bold = |s: &str| -> String {
        if color { s.bold().to_string() } else { s.to_string() }
    };
    let green = |s: &str| -> String {
        if color { s.green().to_string() } else { s.to_string() }
    };
    let yellow = |s: &str| -> String {
        if color { s.yellow().to_string() } else { s.to_string() }
    };
    let red = |s: &str| -> String {
        if color { s.red().to_string() } else { s.to_string() }
    };
    let dim = |s: &str| -> String {
        if color { s.dim().to_string() } else { s.to_string() }
    };

    let (major, minor): (Vec<&DependencyChange>, Vec<&DependencyChange>) =
        changes.modified.iter().partition(|c| {
            let from = c.from_requirement.as_deref().unwrap_or("");
            let to = c.to_requirement.as_deref().unwrap_or("");
            matches!(is_major_bump(from, to), Some(true))
        });

    let write_modified_line = |w: &mut dyn Write, c: &DependencyChange, highlight_to: bool| -> io::Result<()> {
        let from = c.from_requirement.as_deref().unwrap_or("");
        let to = c.to_requirement.as_deref().unwrap_or("");
        let to_painted = if highlight_to { red(to) } else { to.to_string() };
        writeln!(
            w,
            "  {} {} {} -> {} {}",
            yellow("~"),
            yellow(&c.name),
            dim(from),
            to_painted,
            dim(&format!("({})", c.manifest_path)),
        )
    };

    let mut wrote_section = false;

    if !changes.added.is_empty() {
        writeln!(w, "{}", bold("Added:"))?;
        for c in &changes.added {
            let version = c.to_requirement.as_deref().unwrap_or("");
            writeln!(
                w,
                "  {} {} {} {}",
                green("+"),
                green(&c.name),
                version,
                dim(&format!("({})", c.manifest_path)),
            )?;
        }
        wrote_section = true;
    }

    if !major.is_empty() {
        if wrote_section {
            writeln!(w)?;
        }
        writeln!(w, "{}", bold("Modified (major version updates):"))?;
        for c in &major {
            write_modified_line(w, c, true)?;
        }
        wrote_section = true;
    }

    if !minor.is_empty() {
        if wrote_section {
            writeln!(w)?;
        }
        writeln!(w, "{}", bold("Modified (minor version updates):"))?;
        for c in &minor {
            write_modified_line(w, c, false)?;
        }
        wrote_section = true;
    }

    if !changes.removed.is_empty() {
        if wrote_section {
            writeln!(w)?;
        }
        writeln!(w, "{}", bold("Removed:"))?;
        for c in &changes.removed {
            let version = c.from_requirement.as_deref().unwrap_or("");
            writeln!(
                w,
                "  {} {} {} {}",
                red("-"),
                red(&c.name),
                version,
                dim(&format!("({})", c.manifest_path)),
            )?;
        }
    }

    Ok(())
}
