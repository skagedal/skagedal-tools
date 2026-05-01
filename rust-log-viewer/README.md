# rust-log-viewer

A Rust port of [`log-viewer`](../log-viewer/), tracking the same
JSONL-log-as-table use case but rebuilt around [ratatui] for the TUI and a
yet-to-be-chosen toolkit for the GUI front-end. See [`DESIGN.md`](DESIGN.md)
for the framework exploration that motivates this rewrite.

[ratatui]: https://ratatui.rs/

## Status

**Early scaffold.** The TUI shows entries from a JSONL file or stdin in a
three-column table (time / level / message) with vi-style navigation and a
detail view that pretty-prints the full JSON. Most of the TS tool's
functionality — config, profiles, `--exec`, triggers, follow mode, the fields
menu, the browser/GUI front-end — is not implemented yet. The intent is to
land features incrementally on top of this scaffold; see `DESIGN.md` for the
plan.

## Usage

```bash
# from a file
rust-log-viewer path/to/log.jsonl
rust-log-viewer -f path/to/log.jsonl

# from stdin
some-command | rust-log-viewer
some-command | rust-log-viewer -f -
```

## Keys

List view:

| Key | Action |
|-----|--------|
| `j` / `↓` | Next entry |
| `k` / `↑` | Previous entry |
| `g` / `G` | Top / bottom |
| `o` / Enter | Open the selected entry's detail view |
| `q` / Ctrl-C | Quit |

Detail view:

| Key | Action |
|-----|--------|
| `j` / `↓` / `n` | Next entry (stay in detail) |
| `k` / `↑` / `p` | Previous entry |
| `u` / `Esc` / `q` | Back to the list |

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

The top-level `./check` runs the same clippy + test gate.
