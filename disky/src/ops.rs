//! The commands themselves.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, bail};

use crate::config::{ResolvedPair, check_project_name};
use crate::rsync;

pub struct Where {
    pub local: bool,
    pub remote: bool,
}

pub fn list(pair: &ResolvedPair) -> Result<()> {
    let local: BTreeSet<String> = rsync::list_local(&pair.local)?.into_iter().collect();
    let remote: BTreeSet<String> = rsync::list_projects(pair)?.into_iter().collect();

    let all: BTreeSet<&String> = local.union(&remote).collect();
    if all.is_empty() {
        println!(
            "No projects in {} or on {}.",
            pair.local.display(),
            pair.remote_label()
        );
        return Ok(());
    }

    println!("{:<40} WHERE", "PROJECT");
    for name in all {
        let w = Where {
            local: local.contains(name),
            remote: remote.contains(name),
        };
        let mark = match (w.local, w.remote) {
            (true, true) => "both",
            (true, false) => "local only",
            (false, true) => "offloaded",
            (false, false) => unreachable!(),
        };
        println!("{name:<40} {mark}");
    }
    Ok(())
}

pub fn offload(
    pair: &ResolvedPair,
    projects: &[String],
    delete: bool,
    dry_run: bool,
) -> Result<()> {
    let mut failed = Vec::new();
    for project in projects {
        if let Err(e) = offload_one(pair, project, delete, dry_run) {
            eprintln!("disky: {e:#}");
            failed.push(project.clone());
        }
    }
    if !failed.is_empty() {
        bail!("offload failed for: {}", failed.join(", "));
    }
    Ok(())
}

fn offload_one(pair: &ResolvedPair, project: &str, delete: bool, dry_run: bool) -> Result<()> {
    check_project_name(project)?;
    let src = pair.local_project(project);
    let dst = pair.remote_spec(project);

    if !src.exists() {
        bail!("no such project: {}", src.display());
    }
    if src.is_symlink() {
        bail!("refusing to offload a symlink: {}", src.display());
    }
    if !src.is_dir() {
        bail!("not a directory: {}", src.display());
    }

    println!("==> offload: {project}");
    rsync::transfer(pair, &src.to_string_lossy(), &dst, dry_run, true)?;

    if dry_run {
        println!("    (dry run: nothing transferred, verified, or deleted)");
        return Ok(());
    }

    println!("--> verifying by checksum...");
    let diffs = rsync::diff(pair, &src.to_string_lossy(), &dst)?;
    if !diffs.is_empty() {
        for d in &diffs {
            eprintln!("    {d}");
        }
        bail!(
            "verification FAILED for {project} — local copy left untouched ({} difference(s))",
            diffs.len()
        );
    }
    println!("    verified: remote matches local byte for byte.");

    if delete {
        remove_project(&pair.local, &src)?;
        println!("    deleted local copy: {}", src.display());
    } else {
        println!("    kept local copy (pass --delete to reclaim the space).");
    }
    Ok(())
}

/// Delete `target`, but only after proving it really is inside `root`.
fn remove_project(root: &Path, target: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let target = target.canonicalize()?;
    if !target.starts_with(&root) || target == root {
        bail!(
            "refusing to delete {}: not inside {}",
            target.display(),
            root.display()
        );
    }
    std::fs::remove_dir_all(&target)?;
    Ok(())
}

pub fn onload(pair: &ResolvedPair, projects: &[String], force: bool, dry_run: bool) -> Result<()> {
    if projects.is_empty() {
        let remote = rsync::list_projects(pair)?;
        println!("Available on {}:", pair.remote_label());
        if remote.is_empty() {
            println!("  (nothing yet)");
        } else {
            for r in &remote {
                println!("  {r}");
            }
            println!();
            println!("Pull one with: disky onload <name>");
        }
        return Ok(());
    }

    for project in projects {
        check_project_name(project)?;
        let src = pair.remote_spec(project);
        let dst = pair.local_project(project);

        if dst.exists() && !force {
            bail!(
                "{} already exists. Use --force to sync over it, or run \
                 `disky status {}` first.",
                dst.display(),
                project
            );
        }

        println!("==> onload: {project}");
        std::fs::create_dir_all(&pair.local)?;
        // No --delete pulling down: that would silently discard local files.
        rsync::transfer(pair, &src, &dst.to_string_lossy(), dry_run, false)?;
        println!("    now at {}", dst.display());
    }
    Ok(())
}

pub fn status(pair: &ResolvedPair, projects: &[String]) -> Result<()> {
    let names: Vec<String> = if projects.is_empty() {
        rsync::list_local(&pair.local)?
    } else {
        projects.to_vec()
    };
    if names.is_empty() {
        println!("Nothing in {}.", pair.local.display());
        return Ok(());
    }

    let remote: BTreeSet<String> = rsync::list_projects(pair)?.into_iter().collect();

    for project in &names {
        check_project_name(project)?;
        println!("==> {project}");
        let src = pair.local_project(project);
        if !src.is_dir() {
            println!("    local:  absent (offloaded)");
            continue;
        }
        println!("    local:  {} ({})", src.display(), human_size(&src));

        if !remote.contains(project) {
            println!("    remote: absent — not offloaded yet");
            continue;
        }

        let dst = pair.remote_spec(project);
        let diffs = rsync::diff(pair, &src.to_string_lossy(), &dst)?;
        if diffs.is_empty() {
            println!("    remote: in sync (checksum-verified)");
        } else {
            println!("    remote: DIFFERS —");
            for d in &diffs {
                println!("            {d}");
            }
        }
    }
    Ok(())
}

pub fn pairs(config: &crate::config::Config) -> Result<()> {
    if config.pairs.is_empty() {
        println!("No pairs configured.");
        return Ok(());
    }
    println!("{:<16} {:<32} REMOTE", "PAIR", "LOCAL");
    for name in config.pairs.keys() {
        let p = config.resolve(name)?;
        println!(
            "{:<16} {:<32} {}",
            name,
            p.local.display().to_string(),
            p.remote_label()
        );
    }
    Ok(())
}

/// Total size of a directory tree, formatted the way `du -h` would.
fn human_size(path: &Path) -> String {
    fn walk(p: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(p) else {
            return 0;
        };
        entries
            .filter_map(|e| e.ok())
            .map(|e| match e.file_type() {
                Ok(t) if t.is_dir() => walk(&e.path()),
                Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => 0,
            })
            .sum()
    }
    format_bytes(walk(path))
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}B", bytes)
    } else {
        format!("{:.1}{}", value, UNITS[unit])
    }
}

/// Where a bare `disky <cmd>` should act when run from `cwd`.
pub fn default_projects(cwd_project: Option<String>) -> Vec<String> {
    match cwd_project {
        Some(p) => vec![p],
        None => Vec::new(),
    }
}

pub fn require_dir(p: &Path) -> Result<()> {
    if !p.is_dir() {
        bail!("local root {} does not exist", p.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_like_du() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1536), "1.5K");
        assert_eq!(format_bytes(3_650_722_201), "3.4G");
    }

    #[test]
    fn refuses_to_delete_outside_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("studio");
        let inside = root.join("song");
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        assert!(remove_project(&root, &outside).is_err());
        assert!(outside.exists(), "outside dir must survive");

        // The root itself is not a project and must never be removed.
        assert!(remove_project(&root, &root).is_err());
        assert!(root.exists());

        remove_project(&root, &inside).unwrap();
        assert!(!inside.exists());
    }

    #[test]
    fn default_project_comes_from_the_cwd() {
        assert_eq!(default_projects(Some("song".into())), vec!["song"]);
        assert!(default_projects(None).is_empty());
    }
}
