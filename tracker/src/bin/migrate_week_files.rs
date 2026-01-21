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

use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    let dry_run = args.len() > 1 && args[1] == "--dry-run";

    // Find the data directory
    let data_dir = get_data_dir();
    let week_files_dir = data_dir.join("week-files");

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
            // Calculate the correct ISO week
            if let Some(iso_week) = calculate_iso_week(year, week_w) {
                let new_filename = format!("{}-W{:02}.txt", year, iso_week);
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

    // Check for conflicts
    let mut new_names = HashMap::new();
    let mut has_conflicts = false;

    for (_, _, old_name, new_name) in &migrations {
        if let Some(existing_old) = new_names.insert(new_name.clone(), old_name.clone()) {
            if !has_conflicts {
                eprintln!("ERROR: Conflicts detected!");
                has_conflicts = true;
            }
            eprintln!(
                "  '{}' and '{}' would both become '{}'",
                existing_old, old_name, new_name
            );
        }
    }

    if has_conflicts {
        eprintln!("\nMigration aborted due to conflicts.");
        std::process::exit(1);
    }

    // Perform migrations
    println!("Migrating {} file(s):\n", migrations.len());

    for (old_path, new_path, old_name, new_name) in migrations {
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

fn calculate_iso_week(year: i32, week_w: u32) -> Option<u32> {
    // %W format: Week number starting with the first Sunday as the first day of week 1 (00-53)
    // Week 0 contains days before the first Sunday of the year

    // Strategy: Find a representative date in this week and get its ISO week
    // We'll use the middle of the week (Wednesday) to avoid edge cases

    // Find January 1st of the year
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1)?;

    // Calculate when the first Sunday of the year is
    let days_until_sunday = (7 - jan1.weekday().num_days_from_sunday()) % 7;

    // Calculate a representative date in the target week
    let target_date = if week_w == 0 {
        // Week 0: use a date before the first Sunday (e.g., January 1st if it's not Sunday)
        // Use the day before the first Sunday, or Jan 1 if it's earlier
        if days_until_sunday > 0 {
            jan1
        } else {
            // January 1 is a Sunday, so week 0 doesn't really exist in this year
            // This shouldn't happen with valid %W data, but handle it gracefully
            jan1
        }
    } else {
        // For week N (N > 0), go to the first Sunday, then add (N-1) weeks, then add 3 days to get to Wednesday
        jan1.checked_add_signed(chrono::Duration::days(
            (days_until_sunday + (week_w - 1) * 7 + 3) as i64,
        ))?
    };

    Some(target_date.iso_week().week())
}

fn get_data_dir() -> PathBuf {
    // Use etcetera to get the data directory, matching what the tracker app does
    use etcetera::base_strategy::{choose_base_strategy, BaseStrategy};

    let strategy = choose_base_strategy().expect("Could not determine base strategy");
    strategy.data_dir().join("tracker")
}
