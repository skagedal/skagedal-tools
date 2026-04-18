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
- **Interactive TUI mode**: Run with `--interactive` for a terminal UI with navigation and snooze support
  - Navigate with `j`/`k` or arrow keys
  - Search with `/`
  - Snooze a repository for 1 hour with `s`
- **Output**: Prints full paths of repositories with uncommitted changes, one per line (non-interactive mode)

## Requirements

- Rust toolchain (cargo and rustc)
- Git must be installed and available in PATH

## Other implementations

The [`other-implementations/`](other-implementations/) directory contains shell script and Java implementations for reference.
