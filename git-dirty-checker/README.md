# git-dirty-checker

A tool to find git repositories with uncommitted changes (dirty working trees) in subdirectories of specified paths.

## Purpose

Quickly identify which repositories in a collection have uncommitted changes. Useful when maintaining multiple repositories to ensure you haven't forgotten to commit or push changes.

## Building

```bash
cargo build --release
```

The compiled binary will be available at `target/release/git-dirty-checker`.

## Usage

```bash
./target/release/git-dirty-checker /path/to/repos /another/path
```

Or run directly with cargo:

```bash
cargo run --release -- /path/to/repos /another/path
```

## Features

- **Parallel processing**: Uses Rayon to check multiple repositories concurrently
- **Interactive mode**: Run with `--interactive` for an inline UI with navigation and snooze support
  - Navigate with `j`/`k` or arrow keys
  - Search with `/`
  - Snooze a repository for 1 hour with `s`; press `s` again on a snoozed repo to add another hour
  - Unsnooze a repository with `u`
  - Snoozed repos remain visible in grey at the bottom of the list
- **Output**: Prints full paths of repositories with uncommitted changes, one per line (non-interactive mode)

## Integration with `assistant` / `SUGGESTED_CD_FILE`

When the interactive mode selection results in a directory, the tool checks the `SUGGESTED_CD_FILE` environment variable. If set, it writes the selected path to that file and exits with code `10` instead of printing to stdout. This follows the convention used by the `assistant` program, which uses this mechanism to `cd` the calling shell into the selected directory.

## Requirements

- Rust toolchain (cargo and rustc)
- Git must be installed and available in PATH

## Other implementations

The [`other-implementations/`](other-implementations/) directory contains shell script and Java implementations for reference.
