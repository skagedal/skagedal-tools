//! Migration tool to rename week files from %W format (0-indexed) to %V format (1-indexed ISO weeks).
//!
//! This tool scans the tracker data directory for week files and renames them from the old
//! format (using %W which is 0-indexed) to the new format (using %V which uses ISO week numbers).
//!
//! Usage:
//!   cargo run --bin migrate_week_files [--dry-run]
//!
//! Or after building:
//!   ./target/release/migrate_week_files [--dry-run]
//!
//! Options:
//!   --dry-run    Show what would be renamed without actually renaming files
//!
//! RUN THIS ONCE. Old (%W) and new (%G-W%V) filenames have the same shape — there is no way
//! to tell them apart from the filename alone, so a second run will reinterpret the
//! already-migrated files as old-format and shift everything by another week.

use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use tracker::paths::TrackerDirs;

fn main() {
    let args: Vec<String> = env::args().collect();
    let dry_run = args.len() > 1 && args[1] == "--dry-run";

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
            "LIVE (files will be renamed)"
        }
    );
    println!();

    // Scan for week files
    let entries = match fs::read_dir(&week_files_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Error reading directory: {}", e);
            return;
        }
    };

    let mut migrations = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error reading entry: {}", e);
                continue;
            }
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Parse filename: YYYY-WWW.txt (old format with %W)
        if let Some((year, week_w)) = parse_old_format(filename) {
            if let Some(target_date) = representative_date(year, week_w) {
                // Format with %G (ISO week year) and %V (ISO week) so the year and week stay
                // consistent across year boundaries.
                let new_filename = target_date.format("%G-W%V.txt").to_string();
                let new_path = week_files_dir.join(&new_filename);

                if filename != new_filename {
                    migrations.push((
                        path.clone(),
                        new_path,
                        filename.to_string(),
                        new_filename,
                    ));
                }
            }
        }
    }

    if migrations.is_empty() {
        println!("No files need migration.");
        return;
    }

    // Group migrations by destination so we can detect collisions and skip empty duplicates.
    let mut by_target: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (_, _, _, new_name)) in migrations.iter().enumerate() {
        by_target.entry(new_name.clone()).or_default().push(i);
    }

    let mut skipped_empties: Vec<(PathBuf, String, String)> = Vec::new();
    let mut has_conflicts = false;

    for (new_name, indices) in &by_target {
        if indices.len() <= 1 {
            continue;
        }
        // Multiple sources collide on the same destination. If all but one are empty, drop
        // the empties — they carry no data and are an artifact of the buggy old naming.
        let mut non_empty: Vec<usize> = Vec::new();
        let mut empty: Vec<usize> = Vec::new();
        for &i in indices {
            let (path, _, _, _) = &migrations[i];
            let is_empty = fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false);
            if is_empty {
                empty.push(i);
            } else {
                non_empty.push(i);
            }
        }

        if non_empty.len() > 1 {
            if !has_conflicts {
                eprintln!("ERROR: Conflicts detected!");
                has_conflicts = true;
            }
            let names: Vec<&str> = non_empty
                .iter()
                .map(|&i| migrations[i].2.as_str())
                .collect();
            eprintln!(
                "  multiple non-empty files {:?} would all become '{}'",
                names, new_name
            );
        } else {
            // Zero or one non-empty source — keep it (or none) and drop the empty duplicates.
            // If zero non-empty, also drop one of the empties from the migration list so the
            // surviving one takes the destination.
            let to_drop: Vec<usize> = if non_empty.is_empty() {
                empty[1..].to_vec()
            } else {
                empty
            };
            for i in to_drop {
                skipped_empties.push((
                    migrations[i].0.clone(),
                    migrations[i].2.clone(),
                    new_name.clone(),
                ));
            }
        }
    }

    if has_conflicts {
        eprintln!("\nMigration aborted due to conflicts.");
        std::process::exit(1);
    }

    // Filter out the empty duplicates we decided to skip.
    if !skipped_empties.is_empty() {
        let drop_paths: std::collections::HashSet<PathBuf> =
            skipped_empties.iter().map(|(p, _, _)| p.clone()).collect();
        migrations.retain(|(p, _, _, _)| !drop_paths.contains(p));

        println!(
            "Skipping {} empty duplicate file(s) that would collide with another migration:",
            skipped_empties.len()
        );
        for (_, old_name, target) in &skipped_empties {
            println!("  {} (would have become {})", old_name, target);
        }
        println!();
    }

    // Order matters: fs::rename overwrites silently on Unix. If we rename W01->W02 while W02
    // is still a pending source, we destroy W02's data. Repeatedly pick any migration whose
    // destination is neither a pending source nor an unrelated existing file, and do that one.
    let pending_sources: std::collections::HashSet<PathBuf> =
        migrations.iter().map(|(p, _, _, _)| p.clone()).collect();

    // Sanity check: any destination that already exists on disk and isn't itself a pending
    // source means we'd clobber an unrelated file. Abort.
    let mut clobber = Vec::new();
    for (_, new_path, _, new_name) in &migrations {
        if new_path.exists() && !pending_sources.contains(new_path) {
            clobber.push(new_name.clone());
        }
    }
    if !clobber.is_empty() {
        eprintln!("ERROR: these destinations already exist and would be overwritten:");
        for name in &clobber {
            eprintln!("  {}", name);
        }
        eprintln!("\nMigration aborted.");
        std::process::exit(1);
    }

    let ordered = match order_migrations(migrations) {
        Ok(v) => v,
        Err(cycle) => {
            eprintln!("ERROR: rename cycle detected involving:");
            for name in &cycle {
                eprintln!("  {}", name);
            }
            eprintln!("\nMigration aborted.");
            std::process::exit(1);
        }
    };

    println!("Migrating {} file(s):\n", ordered.len());

    for (old_path, new_path, old_name, new_name) in ordered {
        println!("  {} -> {}", old_name, new_name);

        if !dry_run
            && let Err(e) = fs::rename(&old_path, &new_path)
        {
            eprintln!("    ERROR: Failed to rename: {}", e);
        }
    }

    if dry_run {
        println!("\nDry run complete. No files were modified.");
        println!("Run without --dry-run to perform the migration.");
    } else {
        println!("\nMigration complete!");
    }
}

fn parse_old_format(filename: &str) -> Option<(i32, u32)> {
    // Expected format: YYYY-WWW.txt where W is from %W format (00-53)
    if !filename.ends_with(".txt") {
        return None;
    }

    let without_ext = &filename[..filename.len() - 4];
    let parts: Vec<&str> = without_ext.split("-W").collect();

    if parts.len() != 2 {
        return None;
    }

    let year = parts[0].parse::<i32>().ok()?;
    let week = parts[1].parse::<u32>().ok()?;

    Some((year, week))
}

/// Topologically order renames so we never rename onto a file that's still a pending source.
/// Returns Err with the names of files in a cycle if one exists (no valid order possible).
fn order_migrations(
    migrations: Vec<(PathBuf, PathBuf, String, String)>,
) -> Result<Vec<(PathBuf, PathBuf, String, String)>, Vec<String>> {
    let mut remaining = migrations;
    let mut ordered = Vec::with_capacity(remaining.len());

    while !remaining.is_empty() {
        let pending_sources: std::collections::HashSet<PathBuf> =
            remaining.iter().map(|(p, _, _, _)| p.clone()).collect();

        let safe_idx = remaining
            .iter()
            .position(|(_, new_path, _, _)| !pending_sources.contains(new_path));

        match safe_idx {
            Some(i) => ordered.push(remaining.swap_remove(i)),
            None => {
                // No safe move — everything left is in a cycle.
                return Err(remaining.into_iter().map(|(_, _, n, _)| n).collect());
            }
        }
    }

    Ok(ordered)
}

fn representative_date(year: i32, week_w: u32) -> Option<NaiveDate> {
    // %W format: Monday is the first day of the week. Week 1 starts on the first Monday of the
    // year; week 0 covers the days before that (if any).
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1)?;
    let days_until_monday = (7 - jan1.weekday().num_days_from_monday()) % 7;

    if week_w == 0 {
        // Days before the first Monday. Pick Jan 1 — it's always inside week 0 when week 0
        // exists (i.e. when Jan 1 isn't already a Monday).
        Some(jan1)
    } else {
        // Wednesday of week N: first Monday + (N-1) weeks + 2 days. Wednesday avoids the
        // year-boundary edges where Monday/Sunday could fall in the adjacent ISO week year.
        jan1.checked_add_signed(chrono::Duration::days(
            (days_until_monday + (week_w - 1) * 7 + 2) as i64,
        ))
    }
}

