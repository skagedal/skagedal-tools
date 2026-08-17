# Migrations

Two one-time migrations of the week files under the tracker data directory. Both tools take `--dry-run`.

---

# Duration Format Migration

## Background

Durations used to be written as separate hour and minute terms, `* balance 3h 12m`. They are now written in the compound format, `* balance 3:12h` – see the "Durations" section of the README. The old form still parses, so this migration is optional; it exists so an archive of week files reads the same way throughout.

## How to Migrate

1. **Dry run**:
   ```bash
   cargo run --bin migrate_duration_format -- --dry-run
   ```

2. **Perform the migration**:
   ```bash
   cargo run --bin migrate_duration_format
   ```

Running it twice is harmless – durations already in the compound format are left alone.

## Ambiguous Lines

A line like `* balance -9h 26m` is read term by term, as `-9h` plus `26m`, which is `-8:34h`. But it is also exactly what the old report printed for `-9:26h`, and copying that output into a week file was how a balance got carried over by hand – so it most likely means `-9:26h`.

The tool cannot tell these apart, so it leaves such lines alone and lists them at the end. Pass `--fix-negative-minutes` to read them as `-9:26h` instead. Lines where the minutes are zero, `* balance -8h 0m`, mean the same thing either way and are migrated normally.

## Safety

- Only files named `YYYY-Www.txt` are touched; editor autosaves and other files in the directory are left alone.
- Only duration lines are rewritten. Every other line is written back unchanged.
- A line that isn't a duration – a shift, a special day, a comment – is never touched.

---

# Week File Naming Migration

## Background

Prior to this fix, week files were named using the `%W` format specifier, which uses a zero-indexed week numbering system (weeks 00-53). This caused an off-by-one error where the first week of the year was labeled as week 0 instead of week 1.

The fix changes the format to use `%V`, which uses ISO 8601 week numbering (weeks 01-53), where:
- Week 1 is the first week containing a Thursday
- Weeks start on Monday
- Weeks are numbered 01-53

## Migration Required

If you have existing week files, you'll need to rename them to match the new format.

## How to Migrate

### Option 1: Using the Migration Tool (Recommended)

1. **Dry run** (see what would change without making changes):
   ```bash
   cargo run --bin migrate_week_files -- --dry-run
   ```

2. **Perform the migration**:
   ```bash
   cargo run --bin migrate_week_files
   ```

Or, if you've already built the release version:
```bash
./target/release/migrate_week_files
```

### Option 2: Build and Install the Migration Tool

```bash
# Build the migration tool
cargo build --release --bin migrate_week_files

# Run it
./target/release/migrate_week_files
```

## What the Migration Does

The migration tool:
1. Scans your tracker data directory (`~/.local/share/tracker/week-files` by default)
2. Finds all week files with the old naming format
3. Calculates the correct ISO week number for each file
4. Renames the files to use ISO week numbering
5. Detects and prevents conflicts where multiple files would map to the same new name

## Example

Before:
```
2026-W00.txt  (week 0 - days before first Sunday)
2026-W01.txt  (week 1)
2026-W52.txt  (week 52)
```

After:
```
2026-W01.txt  (ISO week 1)
2026-W02.txt  (ISO week 2)
2026-W53.txt  (ISO week 53)
```

## Safety

- The tool checks for naming conflicts before making any changes
- Use `--dry-run` to preview changes before applying them
- The tool will abort if conflicts are detected
- No data is lost or modified - only filenames are changed

## Notes

- **%W format**: Week number with Sunday as first day of week, 00-53
- **%V format**: ISO 8601 week number with Monday as first day of week, 01-53
- Week boundaries may shift slightly due to the different week start days (Sunday vs Monday)
- This is a one-time migration - after updating your tracker installation, you should run this once
