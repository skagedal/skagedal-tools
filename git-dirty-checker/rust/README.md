# Rust Implementation of git-dirty-checker

A parallel Rust implementation to find git repositories with uncommitted changes.

## Features

- **Parallel processing**: Uses Rayon to check multiple repositories concurrently for better performance
- **Fast**: Compiled binary provides excellent performance
- **Simple**: Minimal dependencies and straightforward implementation

## Building

```bash
cargo build --release
```

The compiled binary will be available at `target/release/git-dirty-checker`.

## Usage

```bash
cargo run --release -- /path/to/repos /another/path
```

Or, after building:

```bash
./target/release/git-dirty-checker /path/to/repos /another/path
```

## Output

Outputs the full paths of repositories with uncommitted changes, one per line.

## Requirements

- Rust toolchain (cargo and rustc)
- Git must be installed and available in PATH
