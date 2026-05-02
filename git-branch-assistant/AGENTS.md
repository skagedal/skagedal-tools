# AGENTS.md

This file provides guidance to AI coding assistants (Claude Code, GitHub Copilot, Codex) when working with code in this repository.

## Project Overview

git-branch-assistant is a Rust CLI tool for managing git branches and synchronizing them with upstreams. It provides two main commands:
- `clean`: Cleans up branches in a single repository by analyzing upstream status
- `repos`: Batch management for multiple repositories in subdirectories

## Project Structure & Module Organization

- `src/main.rs` wires CLI parsing (via `clap`) into the domain logic
- Feature flows live in `src/commands/` (`git_clean.rs`, `git_repos.rs`) and should stay thin, delegating to services
- Git orchestration code sits in `src/git/` with shared helpers plus `tests.rs` for low-level fixtures; extend those when touching git plumbing
- Cross-cutting utilities (`fs_utils.rs`, `env.rs`, `task_result.rs`, `ui.rs`) host reusable pieces; prefer augmenting these before adding new ad-hoc helpers
- Installation helpers (`install.sh`, `update`) target local setups; build artifacts land in `target/` as usual for Cargo

## Build and Development Commands

### Building and Linting
```bash
cargo build                                    # Debug build
cargo build --release                          # Release build
cargo fmt                                      # Format code (run before every commit)
cargo clippy --all-targets -- -D warnings      # Lint for regressions
```

### Testing
```bash
cargo test                    # Run all tests
cargo test <test_name>        # Run specific test
cargo test -- --nocapture     # Show println output during tests
```

### Running
```bash
cargo run -- clean              # Run clean command on current directory
cargo run -- clean --path /path # Run clean on specific path
cargo run -- clean --dry        # Dry run (analyze without actions)
cargo run -- repos              # Run repos command on current directory
cargo run -- repos --dry        # Dry run repos command
```

### Installation
```bash
./install.sh ~/.local/bin      # Install binary to custom prefix for manual testing
```

## Architecture

### Core Components

**GitRepo** (`src/git/mod.rs`): Central abstraction for git operations. Executes git commands via `Command::new("git")` in the repository directory.

**GitCleaner** (`src/cleaner.rs`): The core branch management logic. Generic over a `Prompt` trait to enable both interactive UI (DialoguerPrompt) and testing (TestPrompt). Handles each branch based on its upstream status.

**Branch and Upstream** (`src/git/mod.rs`): Data structures representing branch state:
- `Branch`: Contains refname, optional upstream, and optional worktree path
- `Upstream`: Contains upstream name and `UpstreamStatus` enum
- `UpstreamStatus`: Enum with states (Identical, UpstreamIsAheadOfLocal, LocalIsAheadOfUpstream, MergeNeeded, UpstreamIsGone)

**Prompt trait** (`src/ui.rs`): Abstraction for user prompts. Implemented by:
- `DialoguerPrompt`: Real interactive prompts using dialoguer crate
- `DryRunPrompt`: No-op for dry run mode
- `TestPrompt`: Test double with pre-configured selections

### Branch State Machine

The cleaner handles branches based on their state:

1. **No upstream**: Offers to push and create PR, push to create origin, delete, show log, or exit to shell
2. **Identical**: No action needed, continues
3. **Upstream ahead**: Auto-rebases (unless branch checked out in another worktree)
4. **Local ahead**: Prompts to push, show log, exit to shell, or do nothing
5. **Diverged**: Prompts to rebase, show log, delete, exit to shell, or do nothing
6. **Upstream gone**: Prompts to delete, show log, exit to shell, or do nothing

### Worktree Handling

The tool is worktree-aware:
- Tracks which worktree each branch is checked out in
- Redirects to the worktree directory when actions require it
- Offers to delete both worktree and branch when upstream is gone

### External Tools

The tool relies on these external commands:
- `git`: All git operations
- `gh`: Creating pull requests and fetching default branch
- `tig`: Showing git log (optional, used by "Show git log" action)

### Exit Code Convention

When user action is required (branch checkout or worktree redirect), the tool exits with code 10 and writes the suggested directory to a file specified by the environment variable. This allows shell integration to automatically cd to that directory.

## Code Patterns

### Testing Strategy

Tests in `src/cleaner.rs` use the `TestPrompt` implementation to simulate user selections. Tests verify:
- Correct actions for each branch state
- Worktree redirection logic
- Prompts are skipped for identical branches

### Feature Flags

- `timings`: Enable timing instrumentation (optional)

### Error Handling

Uses `anyhow::Result` throughout for error propagation. Git command failures include stderr/stdout context.

## Coding Style & Naming Conventions

- Follow Rust 2024 defaults: four-space indentation, snake_case modules/functions, PascalCase types, SCREAMING_SNAKE_CASE constants
- Keep modules focused; prefer `mod tests` colocated with implementation unless shared fixtures belong in `src/git/tests.rs`
- Handle fallible operations with `anyhow::Result` and provide actionable context via `with_context`
- User-facing prompts live in `ui.rs`; update copy there to keep messaging centralized

## Implementation Notes

When modifying branch handling logic in `cleaner.rs`:
- The `GitCleaner` is generic over `Prompt` for testability
- Each `BranchAction` has a description and corresponding `perform_action` handler
- Worktree checks must happen before operations that assume branch is checked out locally
- Dry run mode should print what would happen and return `TaskResult::Proceed`

When writing tests:
- Favor deterministic unit tests that stub filesystem/git interactions via `tempfile` and helper builders in `src/git/tests.rs`
- Name tests after the scenario (e.g., `cleans_branch_without_upstream`) and document edge cases with comments sparingly
- Add regression tests whenever touching branch comparison logic or multi-repo traversal to prevent silent git changes

## Commit & Pull Request Guidelines

**Commits:**
- Follow existing history: short, imperative commit messages (`Add tests`, `Rename binary`) that describe the change's intent
- Ensure each commit formats (`cargo fmt`) and lints (`cargo clippy -D warnings`) cleanly before pushing

**Pull Requests:**
- Include overview of the user-facing impact and mention of new commands/flags
- Provide test evidence (`cargo test` output) and screenshots/terminal excerpts for interactive flows when relevant
- Cross-link related issues and call out any manual steps (e.g., rerunning `install.sh`) so downstream users know what to expect post-merge
