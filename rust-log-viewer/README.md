# rust-log-viewer

A Rust port of [`log-viewer`](../log-viewer/), with two front-ends
that share one source/config/trigger engine:

- **TUI** ([ratatui][ratatui]) — default.
- **Browser-style GUI** — `-w` / `--web`. Embeds the React app from
  [`../log-viewer/web/`](../log-viewer/web/) into a localhost HTTP
  server inside the binary, then opens a [wry][wry] webview pointing
  at it. The React source is consumed verbatim — same UI as the TS
  tool's `--browser` mode. Behind the `web` Cargo feature (off by
  default; the repo's `./install` enables it for this tool).

The two modes are mutually exclusive at startup. See
[`DESIGN.md`](DESIGN.md) for why these stacks, and [`TODO.md`](TODO.md)
for follow-ups (Tauri migration, co-locating the React app).

[ratatui]: https://ratatui.rs/
[wry]: https://github.com/tauri-apps/wry

## Usage

```bash
# TUI from a file
rust-log-viewer app.jsonl
rust-log-viewer -f app.jsonl

# TUI from stdin
some-command | rust-log-viewer

# TUI from a streaming command — every argv after --exec is passed through
rust-log-viewer --exec kubectl logs -f my-pod

# Pick a profile from the config
rust-log-viewer --profile stern --exec stern --output json my-app

# Web mode (embedded React app in a wry webview)
rust-log-viewer -w app.jsonl
rust-log-viewer -w --exec kubectl logs -f my-pod
```

### Flags

| Flag | Description |
|------|-------------|
| `-f`, `--file <path>` | Read JSONL from a file. Use `-` for stdin. |
| `-c`, `--config <path>` | Path to a config file (overrides `~/.skagedal-tools/rust-log-viewer/config.toml` and `$RUST_LOG_VIEWER_CONFIG`). |
| `--profile <name>` | Activate a profile defined in the config file. |
| `-e`, `--exec <cmd> [args...]` | Run an executable; its stdout is parsed as JSONL. Every argv after `--exec` is forwarded to it, so `--exec kubectl logs -f pod` runs `kubectl logs -f pod`. Put rust-log-viewer's own flags before `--exec`. |
| `-w`, `--web` | Open the React app in a wry webview (only available with the `web` Cargo feature; `./install` enables it). |

Subcommand:

| Subcommand | Description |
|------------|-------------|
| `rust-log-viewer edit-config` | Create the config file if missing, then open it in `$EDITOR` (or `$VISUAL`). |

If no input flag and no positional argument are given but stdin is piped,
`rust-log-viewer` reads stdin.

## Configuration

The config file is **TOML** and lives at
`~/.skagedal-tools/rust-log-viewer/config.toml` by default. Override the
base directory with `SKAGEDAL_TOOLS_HOME`, or set `RUST_LOG_VIEWER_CONFIG`
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

[[profiles]]
name = "stern"

[[profiles.fields]]
name = "time"
from = ["timestamp"]

[[profiles.fields]]
name = "pod"
from = ["podName"]

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
(currently the same set as log-viewer's TS browser mode).

## Cargo features

| Feature | Default | Effect |
|---------|---------|--------|
| `web` | **no** | Compiles the wry-based browser GUI in. Embeds the React app dist via `include_dir!`. Requires the React app to be pre-built (`pnpm run build:web` in `log-viewer/`) and, on Linux, system packages `libgtk-3-dev` + `libwebkit2gtk-4.1-dev`. |

```bash
# Default build — TUI only
cargo build --release

# Build with the embedded web GUI
pnpm --dir ../log-viewer install && pnpm --dir ../log-viewer run build:web
cargo build --release --features web
```

`./install rust-log-viewer` from the repo root runs the React build
and the `--features web` install in one shot.

The `web` feature only adds dependencies; the wry window is created
lazily inside `web::run`, so the TUI startup path doesn't pay any
wry cost beyond the larger binary.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings

# Web feature — needs the React app built first.
pnpm --dir ../log-viewer install && pnpm --dir ../log-viewer run build:web
cargo clippy --features web --all-targets -- -D warnings
cargo test --features web
```

The top-level `./check` runs the default-feature clippy + test gate
(it doesn't try to build with `--features web` so contributors who
don't have GTK/webkit2gtk dev libs installed can still run it).
