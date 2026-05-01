# rust-log-viewer — todo

Things deliberately deferred. Captured here so they don't get lost.

## Tauri instead of wry for the `--web` mode

The `web` front-end currently uses [wry] directly: a TCP server bound to
a free localhost port serves the React app + the `/api/meta` and
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
   at `../log-viewer/web/dist` (or its in-tree successor — see below).
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

## Move the React app into rust-log-viewer/

Today the React source lives at `log-viewer/web/src/` (it's the same
codebase the TS log-viewer's `--browser` mode serves) and our `web`
feature embeds the built `log-viewer/web/dist/`. That's nice for
keeping the Rust port truly source-compatible with the TS tool, but
it's not where this code wants to live long-term:

- The React app and the Rust binary are now the only things that
  talk to each other (the TS Node-side server is a separate
  consumer of the same codebase, but its days are numbered as the
  Rust port reaches parity).
- Touching the React app means cross-tool changes during development.
- The build step (`pnpm run build:web` in `log-viewer/`) is awkward
  to spell from inside `rust-log-viewer/`.

Plan when we pull the trigger:

1. Move `log-viewer/web/` to `rust-log-viewer/web/` (keep the same
   Vite config, same source tree).
2. Add a `rust-log-viewer/web/package.json5` so it's an independent
   pnpm project — no shared root install.
3. Update `include_dir!` to `$CARGO_MANIFEST_DIR/web/dist`.
4. Update `./install`'s rust-log-viewer special case to build inside
   `rust-log-viewer/web/` instead of `log-viewer/`.
5. Decide whether log-viewer's `--browser` mode keeps the React app
   too (symlink? duplicate?) or drops the browser front-end entirely
   in favor of `rust-log-viewer --web`.
