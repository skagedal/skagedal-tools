# rust-log-viewer — design exploration

This document captures the framework-shopping pass requested in
[issue #39](https://github.com/skagedal/skagedal-tools/issues/39): rewrite
[`log-viewer`](../log-viewer/) in Rust, picking sensible Rust-native
replacements for Ink (TUI) and React + Vite (browser).

The starting point is concrete. log-viewer today has two front-ends:

- A TUI built with [Ink][ink] (React for the terminal).
- A browser app served from a Vite dev server, also React, using TanStack
  Table + TanStack Virtual for the list and a popover for the fields menu.

Both share a config layer (JSON5, profiles, triggers), a source layer
(file / stdin / `--exec` subprocess), and an entry model (JSON object
or `{ default_field: line }` for non-JSON input). The Rust rewrite has
to slot in below the same conceptual seams.

> **Update.** The decisions captured below have landed: the TUI is on
> ratatui, the GUI is on iced behind a default-on `gui` Cargo feature,
> and the config layer is **TOML** rather than JSON5 — `toml` is in the
> Rust ecosystem's stdlib-adjacent toolbelt and matches what the rest
> of the repo's Rust tools (`cloudwatch-insights`, `intellij-patch`)
> already use.

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

The MVP in this repo wires up:

- crossterm raw mode + alternate screen, restored on exit.
- A `Table` with header row + selection-highlight + scrolling.
- A detail view that pretty-prints `serde_json` for the selected entry.
- `j/k`, arrows, `g`/`G`, `o`/Enter, `q`, `Ctrl-C`.

What's still missing on the TUI side and tracked as follow-up work:

- Streaming sources (`--exec`, stdin tail). The current loader is
  eager. Switching to a `mpsc::channel` fed by a producer thread and
  drained on each tick is a small change.
- Config file (`~/.skagedal-tools/rust-log-viewer/config.json5`) +
  profiles. `serde_json` already pulls in everything needed; `json5`
  the crate or `serde_json5` would cover the JSON5 dialect.
- Field-row detail navigation (the TS tool lets you select a single
  field and copy it). Trivial with `ratatui`'s `List` + a state.
- Fields menu (`v`) for visibility / reordering.
- Triggers. The TS implementation is a separate runtime that doesn't
  touch the UI; same shape ports cleanly.
- Follow mode (`f`).

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
  (selected entry, follow mode, fields menu) and pairs well with
  Tokio for the streaming source.
- Pop-OS's [libcosmic][libcosmic] is built on iced — if we ever
  wanted a "looks at home on Linux" coat of paint, the path is
  there. (Don't think we need libcosmic for this app; iced alone is
  sufficient and avoids pulling in a desktop-shell dependency.)
- Active development, recent 0.13 release added scrollable tables
  and broke fewer APIs than iced 0.10→0.12.

Cons:

- Tables aren't a first-class widget. We'd build the column layout
  out of `Row` / `Column` / `Scrollable`, which means we own
  virtualization. ~10k rows is the practical ceiling without
  something custom; the TS tool already uses TanStack Virtual for
  this reason.
- Text selection / copy across multiple cells is not a free thing
  in iced; we'd either render each cell as `text_input` (ugly) or
  ship a "copy this row" affordance (matches the TS tool's `c`
  binding).
- No drag-and-drop story for column reordering — would need to
  re-implement with pointer events.

When it's the right call: if we accept that the GUI will feel
toolkit-native rather than "browser-like," and we're willing to
build a virtualized table once. Probably the best fit on the
Rust-purity axis.

[iced]: https://github.com/iced-rs/iced
[libcosmic]: https://github.com/pop-os/libcosmic

### floem

[floem][floem] is a fine-grained reactive toolkit (think SolidJS for
Rust), built on top of [Lapce][lapce]'s rendering stack. It's
rendered with vello/peniko (GPU) and ships a real virtualized
`virtual_stack` widget, plus drag-and-drop primitives.

Pros:

- Has a virtual list out of the box, which is exactly what we need
  for big log streams — would be a clear win over iced for this
  workload.
- Reactive primitives (`RwSignal`, `Memo`) mean the table can
  observe filtered views of the entry list without re-allocating
  every frame.
- Single static binary.

Cons:

- Younger and less stable than iced. APIs churn, fewer tutorials.
  Lapce is the only large user, so anything they don't need is
  thin.
- Docs are light. Discovery happens by reading source.
- Linux/macOS/Windows support; mobile not in scope.

When it's the right call: if we hit performance ceilings on iced
and want `virtual_stack` without writing it. Likely premature
unless we know we want huge scrollback.

[floem]: https://github.com/lapce/floem
[lapce]: https://lapce.dev/

### Slint

[Slint][slint] is a declarative UI toolkit with its own DSL (`.slint`
files compiled at build time) and Rust bindings. It targets desktop,
embedded, and web (via wasm).

Pros:

- The DSL gives a clean separation between view and logic — much
  easier to read at a glance than a deep iced `view()` tree.
- Built-in `ListView`/`StandardListView` with virtualization.
- Mature commercial backing; embedded story is good if that ever
  becomes interesting (it isn't for this tool).

Cons:

- The DSL is a real second language to learn, and the Rust binding
  still feels like an FFI surface in places — string interpolation,
  conversions to/from Slint types, etc.
- Licensing is GPLv3-or-commercial, which is fine for this repo but
  worth flagging.
- Less idiomatic for "small CLI gets a GUI dialog" — best in class
  for app-shaped UIs with lots of screens.

When it's the right call: if we end up with a multi-screen GUI
beyond "table + detail." For this tool, probably overkill.

[slint]: https://github.com/slint-ui/slint

### egui — explicitly out

The issue rules egui out and that's the right call. egui is brilliant
for tools with constantly-changing layouts (debuggers, inspectors,
profilers), but its immediate-mode reflow doesn't suit a long
list-of-rows app: text selection is poor, large tables stutter
unless you opt into `egui_extras::TableBuilder` with manual
virtualization, and the look-and-feel is unmistakably "developer
tool."

## Recommendation

Land the ratatui TUI now (this PR's scaffold), then revisit the GUI
question once the TUI has feature parity with the TS tool. By that
point we'll know how much of the codebase is in the source/config/
trigger layer (front-end-agnostic) vs the UI layer (front-end-bound),
which is the input that should drive the GUI choice.

Tentative ranking, today, for when we're ready:

1. **iced** — best tradeoff between Rust-purity, complexity, and
   long-term maintainability. Accept that we own the virtualized
   table. The Elm architecture is a comfortable fit for this app
   shape.
2. **Tauri** (with a thin Leptos or Yew front-end, dropping the
   React/Vite stack) — fallback if iced's table story becomes a
   blocker. Keeps the door open to feature-parity with the TS
   browser mode but stays in Rust end-to-end.
3. **floem** — reach for it only if iced's virtualization can't keep
   up with a realistic stern/kubectl stream.
4. **Dioxus / Slint** — possible but neither is the obvious win for
   this specific app.

Whichever wins, the source / entry / config / trigger layers should
live in a `lib.rs` (or a sibling crate) so the GUI is a thin shell
on top of the same engine the TUI uses.
