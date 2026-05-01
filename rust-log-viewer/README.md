# rust-log-viewer

A Rust port of [`log-viewer`](../log-viewer/), built around
[ratatui][ratatui] for the TUI and [iced][iced] for the optional GUI. The
two front-ends share a single source/config/trigger engine; they are
mutually exclusive at startup, picked with the `-g` / `--gui` flag.

[ratatui]: https://ratatui.rs/
[iced]: https://iced.rs/

See [`DESIGN.md`](DESIGN.md) for the framework comparison that motivated
the iced choice.

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

# GUI (iced) from any source
rust-log-viewer -g app.jsonl
rust-log-viewer -g --exec kubectl logs -f my-pod
```

### Flags

| Flag | Description |
|------|-------------|
| `-f`, `--file <path>` | Read JSONL from a file. Use `-` for stdin. |
| `-c`, `--config <path>` | Path to a config file (overrides `~/.skagedal-tools/rust-log-viewer/config.toml` and `$RUST_LOG_VIEWER_CONFIG`). |
| `--profile <name>` | Activate a profile defined in the config file. |
| `-e`, `--exec <cmd> [args...]` | Run an executable; its stdout is parsed as JSONL. Every argv after `--exec` is forwarded to it, so `--exec kubectl logs -f pod` runs `kubectl logs -f pod`. Put rust-log-viewer's own flags before `--exec`. |
| `-g`, `--gui` | Open the iced GUI instead of the TUI (only available with the `gui` Cargo feature, on by default). |

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
| `f` | Toggle follow mode (pin selection to the latest entry) |
| `v` | Open the fields menu |
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

GUI: `j`/`k` and arrow keys move; `o` / Enter opens the detail view; `f`
toggles follow; `g`/`G` jump to top/bottom; `q` quits; `Esc` closes
detail. Click an entry row to select it; click a column toggle in the
header to show/hide a column.

## Cargo features

| Feature | Default | Effect |
|---------|---------|--------|
| `gui` | yes | Compiles the iced GUI in. Drops ~30 MB of crates and a chunk of binary size when off. |

```bash
# Default install — TUI + GUI
cargo build --release

# TUI-only build (no iced, no wgpu, no winit)
cargo build --release --no-default-features
```

The `gui` feature only adds dependencies; iced is initialized lazily
inside `gui::run`, so the TUI startup path doesn't pay any iced cost
beyond the larger binary.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings
```

The top-level `./check` runs the clippy + test gate with default features.
