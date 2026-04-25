# log-viewer

View JSONL logs in a TUI or browser, with vi-like navigation and JSON
drill-down.

## Features

- Reads JSONL from a **file**, **stdin**, or the streaming output of an
  **executable command**.
- Two front-ends:
  - **TUI** built with [Ink](https://github.com/vadimdemedes/ink) (default).
  - **Browser** app served by a [Vite](https://vitejs.dev/) dev server
    (`-b` / `--browser`).
- vi-like navigation in both: `j`/`k` (and arrows) move, `u` moves up, `o`
  (or Enter) opens the entry in detail view, `g`/`G` jump to top/bottom,
  `q`/Esc quits or closes the detail.
- Configurable display fields and configurable default field name for
  non-JSON lines (matches the convention from
  [log-jsonify](../log-jsonify/)).

## Install

```bash
pnpm install
pnpm run build
pnpm link --global
```

The repo's `install.sh` does this for you.

## Usage

```bash
# TUI from a file
log-viewer app.jsonl

# TUI from stdin (-f -)
some-command | log-viewer -f -

# TUI from a streaming command
log-viewer --command "kubectl logs -f my-pod"

# Browser mode — prints a localhost URL
log-viewer --browser app.jsonl
log-viewer -b -c "kubectl logs -f my-pod" --port 5174 --open
```

## Examples

The [`examples/`](examples/) folder ships ready-to-go input:

- `examples/sample.jsonl` — a static file with ~30 entries covering
  multiple services, levels, nested objects, stack traces, and a couple
  of plain-text lines so you can see how non-JSON input is wrapped.
- `examples/streaming-logs` — an executable that emits a fake log line
  every ~400 ms (one in ten is plain text). Tweak with `INTERVAL=` and
  `MAX=` env vars.

```bash
# From the log-viewer/ directory:
log-viewer examples/sample.jsonl                  # TUI, file
log-viewer -b examples/sample.jsonl               # browser, file
log-viewer -c ./examples/streaming-logs           # TUI, live stream
log-viewer -b -c ./examples/streaming-logs        # browser, live stream
```

### Flags

| Flag | Description |
|------|-------------|
| `-f`, `--file <path>` | Read JSONL from a file. Use `-` for stdin. |
| `-c`, `--command "<cmd>"` | Run an executable; its stdout is parsed as JSONL. |
| `-b`, `--browser` | Start the browser app instead of the TUI. |
| `-p`, `--port <n>` | Port for the browser server (default `5173`). |
| `--host <h>` | Host for the browser server (default `127.0.0.1`). |
| `--open` | Open the browser automatically (browser mode only). |
| `--init-config` | Write a default config file if missing. |

If no input flag and no positional argument are given but stdin is piped,
`log-viewer` reads stdin.

## Configuration

The config file lives at:

```
~/.skagedal-tools/log-viewer/config.toml
```

Override the base directory with `SKAGEDAL_TOOLS_HOME`, or set
`LOG_VIEWER_CONFIG` to point at a specific file.

`log-viewer --init-config` creates the file with sensible defaults.

```toml
# Field name used to wrap lines that aren't valid JSON. Matches log-jsonify.
default_field = "message"

# Each [[fields]] block is a column shown in the log list. The first key
# in `from` that resolves to a non-empty value on a given entry wins.
[[fields]]
name = "time"
from = ["@timestamp", "timestamp", "time", "ts"]

[[fields]]
name = "level"
from = ["level", "severity", "lvl"]

[[fields]]
name = "message"
from = ["message", "msg", "@message"]
```

## Non-JSON lines

Like [log-jsonify](../log-jsonify/), lines that don't parse as JSON are
treated as a single message under the configured `default_field`:

```
plain text line, not json
```

becomes

```json
{ "message": "plain text line, not json" }
```

so they show up in the same column as your real `message` fields.

## Keyboard

| Key | Action |
|-----|--------|
| `j` / `↓` | Next entry |
| `k` / `↑` | Previous entry |
| `u` | Up one entry |
| `o` / Enter | Open the selected entry (full JSON) |
| `g` / `G` | Top / bottom |
| `Esc` / `u` (in detail) | Back to the list |
| `q` / Ctrl-C | Quit (TUI only) |
