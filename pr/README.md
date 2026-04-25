# pr

A small CLI for managing GitHub pull requests on the current branch.
Wraps the [`gh`](https://cli.github.com/) CLI.

## Building

```bash
cargo build --release
```

The binary will be at `target/release/pr`.

## Usage

### Default command — print the PR URL

```bash
pr
```

Finds the pull request for the current branch in the current repository and
prints its URL on stdout (and nothing else). Exits with code 1 and an error
message on stderr if no PR exists.

### `--create` — create the PR if it doesn't exist

```bash
pr --create
pr --create --toward develop
```

If a PR already exists, prints its URL. Otherwise creates one as a draft and
prints the URL of the new PR. The base branch is the repository's default
branch, or `main` if that can't be determined, unless `--toward` is given.

### `--open` — open the PR URL in the browser

```bash
pr --open
pr --create --open
```

### `pr comments`

Prints the raw JSON returned by
`GET /repos/{owner}/{repo}/issues/{number}/comments` for the PR. Useful for
agents that want to consume comment data programmatically.

### `pr mark-ready`

Marks the PR for the current branch as ready for review (i.e. takes it out of
draft state).

## Testing

```bash
cargo test
```

The integration tests in `tests/integration.rs` exercise the tool end-to-end
by putting a small mock `gh` (`src/bin/mock-gh.rs`) onto `PATH`. The mock
covers just the `gh` surface this tool uses; its behavior is configured via
`MOCK_GH_*` environment variables (see the doc comment at the top of
`mock-gh.rs`). The mock is excluded from `cargo install`, which only installs
the `pr` binary.
