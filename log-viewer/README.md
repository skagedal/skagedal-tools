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

# TUI from a streaming command — every argv after the command is passed through
log-viewer --exec kubectl logs -f my-pod

# Browser mode — prints a localhost URL
log-viewer --browser app.jsonl
log-viewer -b --exec kubectl logs -f my-pod --port 5174    # ⚠ flags AFTER --exec go to kubectl, not log-viewer
log-viewer -b --port 5174 --exec kubectl logs -f my-pod    # put log-viewer flags BEFORE --exec
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
log-viewer --exec ./examples/streaming-logs       # TUI, live stream
log-viewer -b --exec ./examples/streaming-logs    # browser, live stream
```

### Flags

| Flag | Description |
|------|-------------|
| `-f`, `--file <path>` | Read JSONL from a file. Use `-` for stdin. |
| `-e`, `--exec <cmd> [args...]` | Run an executable; its stdout is parsed as JSONL. **Every argv after the command is forwarded to it**, so `--exec kubectl logs -f my-pod` runs `kubectl logs -f my-pod`. Put log-viewer's own flags before `--exec`. |
| `-b`, `--browser` | Start the browser app instead of the TUI. |
| `-p`, `--port <n>` | Port for the browser server (default `5173`). |
| `--host <h>` | Host for the browser server (default `127.0.0.1`). |
| `--no-open` | Skip auto-opening the browser (browser mode opens it by default). |

There's also one subcommand:

| Subcommand | Description |
|------------|-------------|
| `log-viewer edit-config` | Create the config file if missing, then open it in `$EDITOR` (or `$VISUAL`). |

If no input flag and no positional argument are given but stdin is piped,
`log-viewer` reads stdin.

## Configuration

The config file is JSON5 (comments, trailing commas, and unquoted keys
are fine) and lives at:

```
~/.skagedal-tools/log-viewer/config.json5
```

Override the base directory with `SKAGEDAL_TOOLS_HOME`, or set
`LOG_VIEWER_CONFIG` to point at a specific file.

`log-viewer edit-config` creates the file with sensible defaults if it
doesn't exist yet, then opens it in `$EDITOR` (or `$VISUAL`).

```json5
{
  // Field name used to wrap lines that aren't valid JSON. Matches log-jsonify.
  default_field: "message",

  // Each entry is a column shown in the log list. `from` lists candidate keys
  // to read from each JSON entry; the first one with a non-empty value wins.
  fields: [
    { name: "time",    from: ["@timestamp", "timestamp", "time", "ts"] },
    { name: "level",   from: ["level", "severity", "lvl"] },
    { name: "message", from: ["message", "msg", "@message"] },
  ],
}
```

## Triggers

A trigger runs a shell command the first time a configured field takes
on a value `log-viewer` hasn't seen before. The motivating use case is
"make a sound when a new pod is deployed":

```json5
{
  triggers: [
    {
      name: "pod-deployed",
      on_new_value: "podname",
      action: "say new pod {value} deployed",
      startup_delay_ms: 2000,
    },
  ],
}
```

- `on_new_value` is the field to watch (a string, or a list of
  candidate keys like the `from` lists for columns).
- `{value}` and `{field}` in `action` are substituted as **shell-quoted**
  strings, so values with spaces or quotes are safe.
- `startup_delay_ms` suppresses the action for that long after start.
  Values seen during the delay are still recorded as "already seen", so
  when log-viewer attaches to e.g. `kubectl logs`, the existing pods
  don't all fire the trigger — only genuinely new ones do.

The action runs with these env vars set:

| Var | Value |
|-----|-------|
| `LOG_VIEWER_VALUE` | the new value |
| `LOG_VIEWER_FIELD` | the first key in `on_new_value` |
| `LOG_VIEWER_TRIGGER` | the trigger's `name` |
| `LOG_VIEWER_ENTRY` | the entry's full JSON |

You can have multiple `[[triggers]]` blocks; each tracks its own set of
seen values independently.

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

In the list:

| Key | Action |
|-----|--------|
| `j` / `↓` | Next entry |
| `k` / `↑` | Previous entry |
| `u` | Up one entry |
| `o` / Enter | Open the selected entry (full JSON) |
| `f` | Toggle **follow** mode — pin selection to the latest entry as new ones arrive. Any navigation key turns it back off. |
| `g` / `G` | Top / bottom |
| `q` / Ctrl-C | Quit (TUI only) |

The TUI runs in the terminal's alternate screen buffer (full-screen),
so it doesn't pollute your scrollback; on quit, the previous terminal
contents are restored.

In the detail view:

| Key | Action |
|-----|--------|
| `j` / `↓` | Next entry (stay in detail) |
| `k` / `↑` | Previous entry (stay in detail) |
| `c` | Copy the entry's JSON to the system clipboard |
| `u` / `Esc` / `q` | Back to the list |
