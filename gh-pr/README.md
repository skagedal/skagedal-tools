# gh-pr

A small CLI for managing GitHub pull requests on the current branch.
Wraps the [`gh`](https://cli.github.com/) CLI.

The name avoids clashing with the POSIX `pr(1)` text-formatting utility, and
matches `gh`'s extension-naming convention.

## Building

```bash
cargo build --release
```

The binary will be at `target/release/gh-pr`.

## Usage

### Default command — print the PR URL

```bash
gh-pr
```

Finds the pull request for the current branch in the current repository and
prints its URL on stdout (and nothing else). Exits with code 1 and an error
message on stderr if no PR exists.

### `--create` — create the PR if it doesn't exist

```bash
gh-pr --create
gh-pr --create --toward develop
```

If a PR already exists, prints its URL. Otherwise creates one as a draft and
prints the URL of the new PR. The base branch is the repository's default
branch, or `main` if that can't be determined, unless `--toward` is given.

### `--open` — open the PR URL in the browser

```bash
gh-pr --open
gh-pr --create --open
```

### `gh-pr comments`

Prints all comments on the PR:

- top-level conversation comments
- review summaries — the body a reviewer typed when submitting a review
  (e.g. an `APPROVED` or `CHANGES_REQUESTED` review with a message), shown
  with the review state
- inline review comments grouped into threads

By default, comments in resolved review threads are hidden.

Uses a single GraphQL query against `pullRequest.comments`,
`pullRequest.reviews`, and `pullRequest.reviewThreads`. (Going via REST
would require three endpoints — `/issues/{n}/comments`,
`/pulls/{n}/reviews`, `/pulls/{n}/comments` — and resolved state still
wouldn't be available, since the REST API doesn't expose it.)

Flags:

- `--format <text|json>` — defaults to `text`, a layout readable by both
  humans and agents. `json` emits the raw GraphQL response unmodified.
- `--resolved` — include comments belonging to resolved review threads.
  Has no effect on `--format json`, which is always unfiltered.

### `gh-pr mark-ready`

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
the `gh-pr` binary.
