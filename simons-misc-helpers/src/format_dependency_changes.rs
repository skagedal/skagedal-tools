use crossterm::style::Stylize;
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

    if !changes.modified.is_empty() {
        if wrote_section {
            writeln!(w)?;
        }
        writeln!(w, "{}", bold("Modified:"))?;
        for c in &changes.modified {
            let from = c.from_requirement.as_deref().unwrap_or("");
            let to = c.to_requirement.as_deref().unwrap_or("");
            writeln!(
                w,
                "  {} {} {} -> {} {}",
                yellow("~"),
                yellow(&c.name),
                dim(from),
                to,
                dim(&format!("({})", c.manifest_path)),
            )?;
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
