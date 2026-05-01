# log-viewer — design exploration

This document captures the framework-shopping pass requested in
[issue #39](https://github.com/skagedal/skagedal-tools/issues/39):
rewrite log-viewer in Rust, picking sensible Rust-native replacements
for Ink (TUI) and React + Vite (browser).

The starting point was a TypeScript log-viewer with two front-ends:

- A TUI built with [Ink][ink] (React for the terminal).
- A browser app served from a Vite dev server, also React, using
  TanStack Table + TanStack Virtual for the list and a popover for
  the fields menu.

Both shared a config layer (JSON5, profiles, triggers), a source
layer (file / stdin / `--exec` subprocess), and an entry model (JSON
object or `{ default_field: line }` for non-JSON input). The Rust
rewrite had to slot in below the same conceptual seams.

> **Update.** What landed differs from the original recommendation
> below. The Rust crate is now `log-viewer` (the original TS package
> is folded into [`browser/`](browser/) and only its React app is
> still load-bearing). Two front-ends:
>
> - The **TUI** on ratatui, with full feature parity with the TS tool
>   (streaming sources, profiles, triggers, follow mode, fields menu,
>   field-row detail view with `t` toggle and `c` copy via OSC 52,
>   fuzzy filter on `/`).
> - A **webview-embedded React app** behind the `web` Cargo feature
>   (off by default): the React app under `browser/web/` is built by
>   Vite (driven by `build.rs` so `cargo build --features web` does
>   it automatically), embedded into the Rust binary via
>   `include_dir!`, and served from a hand-rolled localhost HTTP
>   server inside the binary that exposes the same `/api/meta` and
>   `/api/stream` SSE contract the original TS browser server used.
>   A wry webview points at that URL. The React code is consumed
>   verbatim.
>
> An iced-based native GUI was prototyped and then removed — it added
> a lot of dependency weight (iced + winit + wgpu + tokio) for a
> middle-ground UX that wasn't a clear win over the embedded React
> app, which already does the table well.
>
> The config layer is **TOML** rather than JSON5 — `toml` is in the
> Rust ecosystem's stdlib-adjacent toolbelt and matches what the rest
> of the repo's Rust tools (`cloudwatch-insights`, `intellij-patch`)
> already use. The TS sub-package under `browser/` still reads the
> original JSON5 config when used as a Vite dev server.

[ink]: https://github.com/vadimdemedes/ink

## TUI front-end — ratatui

[ratatui][ratatui] is the obvious choice and the issue calls it out
explicitly. It's the actively-maintained successor to `tui-rs`, ships
crossterm/termion/termwiz backends, and has a `Table` widget with a
built-in selection state — exactly what the existing TUI needs. The
reactivity model is immediate-mode (rebuild the frame each tick from
app state), which is a different mental model from Ink's
React-style components but is well-suited to a small data app like
this one. No realistic alternative; the only call to make is the
backend (crossterm) and the layout primitives.

[ratatui]: https://ratatui.rs/

## Browser / GUI front-end — pick one

The TS tool's browser front-end exists for two reasons: (a) people
who want a list they can scroll with the trackpad and copy text from
a real browser selection, and (b) the React/HTML/CSS stack is simply
nice for tabular UI. Reproducing both points in Rust is the part that
needs an actual decision. The issue lists the candidates; here's what
each one means in practice for this app.

### Tauri + a web stack

[Tauri][tauri] is the closest 1:1 match for the TS tool's browser
mode: keep the front-end in HTML/CSS/JS (or a Rust→Wasm UI like
Leptos / Yew), bundle it into a desktop app, talk to a Rust backend
over IPC. The TanStack Table layout could even be reused verbatim.

Pros:

- Lowest-risk path for porting the existing browser UI. We could
  even keep the React code initially and just swap the Vite-served
  source+config bridge for Tauri commands.
- Webview means real text selection, real copy/paste, real
  scrollbars, real CSS — all of which are awkward in native toolkits.
- Single binary distribution with code-signing stories that already
  exist.

Cons:

- It's the heaviest option by far. Pulling in a Node/JS toolchain
  defeats some of the appeal of "rewrite in Rust." Even with a
  Rust→Wasm framework, the runtime is still a webview.
- Hot-reload during development is fine but adds moving parts
  (Vite + Tauri dev mode).
- Platform webview quirks (WKWebView vs WebView2 vs WebKitGTK)
  occasionally bite.

When it's the right call: if we want feature parity with the TS
browser mode and don't want to reimplement a virtualized table.

[tauri]: https://tauri.app/

### Dioxus

[Dioxus][dioxus] is "React for Rust," with multiple renderers
(desktop via webview, native via Blitz, web, mobile, TUI). For our
use case, Dioxus Desktop is essentially Tauri-with-Dioxus-on-top:
same webview substrate, but the UI code is Rust JSX-like macros.

Pros:

- Familiar React mental model, easy to map the existing TS UI onto
  it (hooks, components, controlled inputs).
- One codebase can target desktop and the web — useful if we ever
  want to keep a "browser mode" alongside the desktop binary.
- Built-in virtualization story is improving (`use_list` etc.) but
  not as battle-tested as TanStack Virtual.

Cons:

- Same webview footprint as Tauri.
- Smaller community than Tauri or React; some ecosystem gaps
  (drag-and-drop reordering, popovers) that the TS tool currently
  uses for free from the DOM.

When it's the right call: if we like the React shape but want to be
fully in Rust without writing TS at all.

[dioxus]: https://dioxuslabs.com/

### iced

[iced][iced] is an Elm-architecture native GUI toolkit, GPU-rendered
(wgpu) on macOS / Linux / Windows. It's the closest spiritual match
to ratatui from the immediate-mode-ish, message-passing side: state
+ messages + view + update.

Pros:

- Truly native, no webview, single static binary.
- Elm/Redux model is a natural fit for the kinds of state we manage
  (selected entry, follow mode, fields menu).

Cons:

- Tables aren't a first-class widget. We'd build the column layout
  out of `Row` / `Column` / `Scrollable`, which means we own
  virtualization. ~10k rows is the practical ceiling without
  something custom; the TS tool already uses TanStack Virtual for
  this reason.
- Text selection / copy across multiple cells is not a free thing
  in iced; we'd either render each cell as `text_input` (ugly) or
  ship a "copy this row" affordance.
- No drag-and-drop story for column reordering.
- Heavy dep tree (iced + winit + wgpu + tokio) for a UX that doesn't
  meaningfully beat the TUI for this app shape.

We tried iced and pulled it back out — the prototype worked but it
didn't earn its keep alongside the TUI and the embedded React app.

[iced]: https://github.com/iced-rs/iced

### floem / Slint / egui

[floem][floem] (fine-grained reactive, has a virtualized list),
[Slint][slint] (DSL + Rust bindings, multi-screen apps), and egui
(immediate-mode debugger/inspector vibes) all considered briefly.
None of them shifted the calculus once we'd decided we wanted the
exact React UX rather than a native rebuild — at that point the
right tool is whatever puts a webview on the React app with the
least ceremony. egui in particular is a poor fit for a long
list-of-rows app: text selection is awkward and large tables need
manual virtualization.

[floem]: https://github.com/lapce/floem
[slint]: https://github.com/slint-ui/slint

## What shipped

Two front-ends:

- **TUI on ratatui.** Default. Streaming sources, profiles, triggers,
  follow mode, fields menu, per-field detail with `t` toggle and `c`
  copy via OSC 52, fuzzy filter on `/`. crossterm raw mode + alt
  screen, restored on exit. ~50 unit tests cover entry parsing,
  navigation, follow-mode pinning, fields-menu reorder, detail
  toggle, trigger substitution + dedupe, config profile resolution,
  `--exec` argv extraction, fuzzy match.
- **Embedded React app via wry.** Behind the `web` Cargo feature
  (off by default; the repo's `./install` enables it). The React
  source under `browser/web/` is built by Vite (driven from
  `build.rs` so `cargo build --features web` does it automatically),
  embedded with `include_dir!`, and served from a hand-rolled
  localhost HTTP server (`std::net::TcpListener`, one thread per
  connection, ~250 lines including SSE). A [wry] window points at
  the URL.

Architecture seams kept clean: `source`, `config`, `triggers`, `entry`
are all front-end-agnostic. `ui::run` (TUI) and `web::run` (wry) are
thin shells over the same engine.

[wry]: https://github.com/tauri-apps/wry

See [`TODO.md`](TODO.md) for the planned migration to Tauri (when we
want bundled signed installers) and for the eventual retirement of
the in-tree TS browser CLI once the Rust binary's `--web` mode
supports React hot-reload directly.
