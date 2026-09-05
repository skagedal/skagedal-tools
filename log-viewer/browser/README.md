# log-viewer (browser package)

The React app for log-viewer's `--web` / browser-style view, plus a
small Vite-backed Node CLI used to develop it.

For everyday log viewing, use the Rust `log-viewer` binary (`../`)
— its TUI is the default front-end, and `-w` / `--web` opens the
React app embedded in the binary.

## Layout

- `web/` — the Vite project root for the React app. `web/src/`
  is the React source, `web/dist/` (gitignored) is the build
  output.
- `src/` — the Node CLI that, in dev, runs the same `web/` app
  through a Vite dev server with the same `/api/meta` and
  `/api/stream` SSE endpoints the Rust binary's `--web` mode
  serves. Use this to work on the React app with hot-reload.

The Rust crate's `build.rs` runs `pnpm install && pnpm run build:web`
in this directory whenever it builds with `--features web`, so the
React app is always part of the Rust build pipeline.

## Working on the React app

```bash
# From this directory:
pnpm install        # one-time setup

# Vite dev server with hot reload, fed from a JSONL file:
pnpm dev ../examples/sample.jsonl

# Vite dev server fed from a streaming command:
pnpm dev -- --exec ../examples/streaming-logs
```

The Node CLI accepts the same `-f`, `--exec`, `--profile`, `--port`,
`--host`, and `--no-open` flags it always did — see
[`src/index.ts`](src/index.ts).

To produce a static build (what the Rust binary embeds):

```bash
pnpm run build:web
# → web/dist/{index.html, assets/...}
```

## Configuration

The Node CLI reads `~/.config/skagedal-tools/log-viewer/config.json5` (JSON5)
— the same path and format the original TS log-viewer used. Note that
the Rust binary reads a TOML config at
`~/.config/skagedal-tools/log-viewer/config.toml`. The two configs are
parallel during the transition and can hold the same fields/profiles/
triggers if you want.

See [`../README.md`](../README.md) for the full config reference (the
field/profile/trigger model is the same).

## Why this still exists

The Rust binary's `--web` mode embeds a pre-built copy of `web/dist/`
and serves it from a hand-rolled localhost HTTP server. That works
great for users but is awkward for developing the React app, since
every change needs `cargo build --features web`. The Vite dev server
in this package — invoked via `pnpm dev` — gives the usual
hot-reload loop instead. Once you're happy with a change, the
`build.rs`-driven rebuild picks it up the next time the Rust binary
is built.

See [`../TODO.md`](../TODO.md) for the longer-term plan to drop the
Node CLI entirely once the Rust binary's `--web` mode supports
hot-reload directly.
