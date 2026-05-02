# log-viewer

View JSONL logs in a TUI or in a webview-embedded React app, with
vi-like navigation and JSON drill-down.

Two front-ends, one source / config / trigger engine:

- **TUI** ([ratatui][ratatui]) — default. Streaming sources, profiles,
  triggers, follow mode, fields menu, per-field detail view with `t`
  toggle and `c` copy via OSC 52, fuzzy filter on `/`.
- **Embedded React app** — `-w` / `--web`. The React app under
  [`browser/`](browser/) is built by Vite and embedded into the
  binary; a hand-rolled localhost HTTP server serves it inside a
  [wry][wry] webview. Behind the `web` Cargo feature (off by
  default; the repo's `./install` enables it).

The two modes are mutually exclusive at startup. See
[`DESIGN.md`](DESIGN.md) for why these stacks, and [`TODO.md`](TODO.md)
for follow-ups (Tauri migration, retiring the in-tree TS browser CLI).

[ratatui]: https://ratatui.rs/
[wry]: https://github.com/tauri-apps/wry

## Usage

```bash
# TUI from a file
log-viewer app.jsonl
log-viewer -f app.jsonl

# TUI from stdin
some-command | log-viewer

# TUI from a streaming command — every argv after --exec is passed through
log-viewer --exec kubectl logs -f my-pod

# Pick a profile from the config
log-viewer --profile stern --exec stern --output json my-app

# Web mode (embedded React app in a wry webview)
log-viewer -w app.jsonl
log-viewer -w --exec kubectl logs -f my-pod
```

### Flags

| Flag | Description |
|------|-------------|
| `-f`, `--file <path>` | Read JSONL from a file. Use `-` for stdin. |
| `-c`, `--config <path>` | Path to a config file (overrides `~/.skagedal-tools/log-viewer/config.toml` and `$LOG_VIEWER_CONFIG`). |
| `--profile <name>` | Activate a profile defined in the config file. |
| `-e`, `--exec <cmd> [args...]` | Run an executable; its stdout is parsed as JSONL. Every argv after `--exec` is forwarded to it, so `--exec kubectl logs -f pod` runs `kubectl logs -f pod`. Put log-viewer's own flags before `--exec`. |
| `-w`, `--web` | Open the React app in a wry webview (only available with the `web` Cargo feature; `./install` enables it). |

Subcommand:

| Subcommand | Description |
|------------|-------------|
| `log-viewer edit-config` | Create the config file if missing, then open it in `$EDITOR` (or `$VISUAL`). |

If no input flag and no positional argument are given but stdin is piped,
`log-viewer` reads stdin.

## Configuration

The config file is **TOML** and lives at
`~/.skagedal-tools/log-viewer/config.toml` by default. Override the
base directory with `SKAGEDAL_TOOLS_HOME`, or set `LOG_VIEWER_CONFIG`
to point at a specific file.

```toml
# Field name used to wrap lines that aren't valid JSON.
default_field = "message"

[[fields]]
name = "time"
from = ["@timestamp", "timestamp", "time", "ts"]

[[fields]]
name = "level"
from = ["level", "severity", "lvl"]

[[fields]]
name = "message"
from = ["message", "msg", "@message"]

# Optional. For each name listed, if an entry has that key with an
# object value (or a stringified-JSON object), the inner keys are
# merged up and the parent key is dropped. Inner keys win on collision.
flatten_fields = []

# Optional. When set, each row in the list view is colored by a hash
# of the named field's value — like stern's per-pod coloring. Rows
# whose level is WARN/ERROR/FATAL still get the level color (yellow/
# red) so they stand out. Leave unset to disable.
# color_by_field = "pod"

[[profiles]]
name = "stern"
# Stern's --output extjson nests the app log under "message".
flatten_fields = ["message"]
color_by_field = "pod"

[[profiles.fields]]
name = "pod"
from = ["pod"]

[[triggers]]
name = "pod-deployed"
on_new_value = "podname"   # or a list: ["podname", "podName"]
action = "say new pod {value} deployed"
startup_delay_ms = 2000
```

`{value}` and `{field}` in `action` are substituted as shell-quoted
strings. Action env vars: `LOG_VIEWER_VALUE`, `LOG_VIEWER_FIELD`,
`LOG_VIEWER_TRIGGER`, `LOG_VIEWER_ENTRY`.

## Keyboard

List view:

| Key | Action |
|-----|--------|
| `j` / `↓` | Next entry |
| `k` / `↑` / `u` | Previous entry |
| PgDn / PgUp | Page down / up |
| `g` / `G` | Top / bottom |
| `o` / Enter | Open the selected entry's detail view |
| `/` | Focus the fuzzy filter input |
| `f` | Toggle follow mode (pin selection to the latest entry) |
| `v` | Open the fields menu |
| `Esc` (when filter is set) | Clear the filter |
| `q` / Ctrl-C | Quit |

Detail view:

| Key | Action |
|-----|--------|
| `j` / `↓` | Next field row |
| `k` / `↑` | Previous field row |
| `n` / `p` | Next / previous entry (stay in detail) |
| `t` | Toggle the selected field's column visibility in the list |
| `c` | Copy the selected field's value (or the whole entry on the header row) via OSC 52 |
| `v` | Open the fields menu |
| `u` / `Esc` / `q` | Back to the list |

Fields menu:

| Key | Action |
|-----|--------|
| `j` / `↓`, `k` / `↑` / `u` | Move the cursor |
| Space / `t` | Toggle the highlighted field's visibility |
| `J` / `K` | Move the highlighted field down / up |
| `v` / `q` / `Esc` | Close the menu |

Filter input (when focused):

| Key | Action |
|-----|--------|
| any printable | Append to the filter |
| Backspace | Remove the last char |
| Ctrl-W | Delete the word to the left |
| Ctrl-U | Clear the filter and unfocus |
| Enter / Esc | Unfocus the input (filter stays applied) |

In `--web` mode the keyboard map is whatever the React app implements
(see [`browser/web/src/`](browser/web/src/)).

## Cargo features

| Feature | Default | Effect |
|---------|---------|--------|
| `web` | **no** | Compiles the wry-based webview front-end in. Embeds the React app dist via `include_dir!`. The crate's `build.rs` runs `pnpm install && pnpm run build:web` in [`browser/`](browser/) automatically, so a fresh `cargo build --features web` produces an up-to-date dist. Linux additionally needs system packages `libgtk-3-dev` + `libwebkit2gtk-4.1-dev` and `pnpm` on PATH. |

```bash
# Default build — TUI only, no pnpm/Vite required
cargo build --release

# Build with the embedded webview front-end
cargo build --release --features web
```

`./install log-viewer` from the repo root invokes
`cargo install --features web` and `build.rs` handles the React build.

The `web` feature only adds dependencies; the wry window is created
lazily inside `web::run`, so the TUI startup path doesn't pay any
wry cost beyond the larger binary.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings

# Web feature — pnpm needs to be on PATH; build.rs handles the React build.
cargo clippy --features web --all-targets -- -D warnings
cargo test --features web

# Working on the React app itself (Vite hot-reload):
cd browser
pnpm install
pnpm dev ../examples/sample.jsonl    # or `pnpm dev -- --exec ../examples/streaming-logs`
```

The top-level `./check` runs the default-feature Rust clippy + test
gate, plus `pnpm run check` inside [`browser/`](browser/) to type-check
and lint the React app. It doesn't try to build the Rust crate with
`--features web`, so contributors who don't have GTK/webkit2gtk dev
libs installed can still run it.
