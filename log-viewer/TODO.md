# log-viewer — todo

Things deliberately deferred. Captured here so they don't get lost.

## Tauri instead of wry for the `--web` mode

The `web` front-end currently uses [wry] directly: a TCP server bound
to a free localhost port serves the React app + the `/api/meta` and
`/api/stream` SSE endpoints, and a wry window navigates to that URL.
That's the smallest moving-parts version of the same architecture
[Tauri] gives you.

What we'd gain from switching to Tauri:

- **Bundling and code-signing.** Tauri's CLI produces signed `.app`,
  `.dmg`, `.msi`, etc. with auto-updater plumbing. wry stops at the
  webview; packaging is on you.
- **First-class IPC** between the webview and the Rust side
  (`tauri::command`, events). We don't need this today because the
  React app talks to the embedded HTTP server, but if we ever want
  the React app to call into Rust directly (file picker, native
  notifications, native menus), Tauri gives us that for free.
- **Plugin ecosystem** — official plugins for clipboard, dialog,
  notification, shell, fs, updater.

Migration sketch when/if we want it:

1. Add `tauri = "2"` and `tauri-build = "2"` to `Cargo.toml`, keeping
   `wry` only as a dev fallback (or dropping it).
2. Add `tauri.conf.json` next to `Cargo.toml`. `frontendDist` points
   at `browser/web/dist`.
3. Replace the embedded HTTP server with Tauri's asset protocol
   (`tauri://localhost/`). The React app's `fetch("/api/meta")` and
   `new EventSource("/api/stream")` would need a thin abstraction
   that swaps to `invoke("get_meta")` and a Tauri event listener for
   `entry`. That breaks the "exact React app, untouched" promise we
   have today.
4. Update `./install` to invoke `cargo tauri build` instead of
   `cargo install`.

Net call: stay on wry until either (a) we want bundled installers, or
(b) we want native APIs the webview can't hit.

[wry]: https://github.com/tauri-apps/wry
[Tauri]: https://tauri.app/

## Retire the in-tree TS browser CLI

[`browser/`](browser/) ships a tiny Node CLI (`browser/src/index.ts`)
on top of the React app, used as a Vite dev server with hot reload
when working on the React UI. Once the Rust binary's `--web` mode
supports a hot-reload dev path — either by proxying a Vite dev server
during `cargo run --features web` or by serving the React sources
directly — the Node CLI has no reason to exist. At that point:

1. Delete `browser/src/`, `browser/test/`, the gunshi/json5/tsx deps,
   and the related lint/test scripts.
2. Keep `browser/web/`, `browser/package.json5` (with just the React
   deps + `build:web`), `browser/eslint.config.js`, `browser/examples/`.
3. `./check`'s `pnpm run check` step in `browser/` collapses to
   `pnpm exec tsc -p web --noEmit && eslint web`.

Until then the Node CLI is a useful seam for iterating on the React
app without rebuilding the Rust binary.

## TUI / web feature parity in either direction

Today the TUI has a few capabilities the React app lacks (OSC 52
clipboard copy, the Ctrl-W word-delete in the filter input) and the
React app has a few the TUI lacks (drag-to-reorder columns, a fields
popover with checkboxes). Worth a pass to align them once the
restructure dust settles.
