# chrome-page-notes

Attach your own notes to the pages you visit in Chrome, stored as plain
markdown files in an Obsidian vault.

- Clicking the toolbar icon opens a popup for the current tab. If a note
  already exists for that URL, its content is shown inline (as plain text,
  not rendered) with an "Open in Obsidian" button — which also brings the
  Obsidian app to the foreground. Otherwise, a "Create note" button creates
  one.
- The toolbar icon shows a ✓ badge whenever the current tab has a note, kept
  up to date by a background service worker as you navigate or switch tabs.
  If a request fails, it shows a "!" badge instead, and the popup offers an
  "Open Obsidian" button.

The tool is two halves that ship as one binary:

- `extension/` — the Chrome extension. Chrome's extension APIs have no
  direct file system access, so all it does is talk to the host below.
- `src/` — a Rust CLI (`chrome-page-notes`, a member of this repo's Cargo
  workspace) that is also the extension's [native messaging
  host](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging).
  It reads and writes note files directly in the vault folder on disk — no
  dependency on the `obsidian` CLI or the app being open at all. The one
  exception is "Open in Obsidian", which does need the app: it goes through
  the `obsidian://open?vault=...&file=...` URL scheme (handled by the app
  itself), which launches the app if needed and reliably lands on the right
  vault and note.

(An earlier version shelled out to the [`obsidian`
CLI](https://obsidian.md/plugins?id=cli) for everything, but its `vault=`
targeting option turned out to be silently ignored — every command actually
operated on whichever vault window happened to be focused, which both
under-reported notes and once created a note in the wrong vault entirely.)

## Setup

Install the binary the same way as every other tool in this repo, then let
it register itself with Chrome:

```bash
./install chrome-page-notes    # from the repo root
chrome-page-notes register
```

`register` unpacks its built-in copy of the extension to
`~/.local/share/skagedal-tools/chrome-page-notes/extension`, writes the
native messaging host manifest to `~/Library/Application
Support/Google/Chrome/NativeMessagingHosts/`, and prints the steps for
loading the extension in Chrome (`chrome://extensions` → "Developer mode" →
"Load unpacked"). It also points out the config file below if you don't
have one yet.

Re-run both after changing the extension or the host. The host manifest
embeds an absolute path to the binary, so re-run `register` if the binary
moves too.

### Working on the extension

`register --dev` skips unpacking and points Chrome at `extension/` in the
checkout the binary was built from, so editing `popup.js` and friends only
needs a click on the extension's reload icon rather than a reinstall.
`register --dev <dir>` names the directory explicitly, for when the repo
has moved since the binary was built.

## Commands

| Command | What it does |
|---------|--------------|
| `chrome-page-notes register [--dev [DIR]]` | Register the host with Chrome and print how to load the extension |
| `chrome-page-notes host` | Serve the native messaging protocol on stdin/stdout |

Chrome runs `host` itself, so there's rarely a reason to invoke it by hand.
Its host manifest has no way to pass arguments — Chrome runs the binary
bare and hands it the calling extension's origin as `argv[1]` — so the
binary treats a leading `chrome-extension://…` argument as meaning "serve",
and the `host` subcommand only exists for invoking the same thing manually.

The extension's `manifest.json` has a fixed `"key"` field, which pins its
extension ID to `jbgofjilflakfjbenbgpppajapiffphn` no matter where it's
loaded unpacked from — that ID is also a constant in `src/main.rs`, which
is what lets the host manifest name it without per-machine edits.

## Configuration

The host reads `~/.config/skagedal-tools/chrome-page-notes/config.toml`
(or `$XDG_CONFIG_HOME/skagedal-tools/chrome-page-notes/config.toml`), per this repo's
[convention](../AGENTS.md#per-tool-config-and-state-directories):

```toml
vault_path = "/Users/you/Library/Mobile Documents/iCloud~md~obsidian/Documents/obsidian-notes"  # required — absolute path to the vault folder
folder = "webnotes"                             # optional, defaults to "webnotes"
debug = false                                   # optional, defaults to false
```

`vault_path` must point directly at the vault's folder on disk (find it via
Obsidian's own "Open another vault" list, or `obsidian vaults verbose` if
you have the CLI installed) — the vault's name, used for the `obsidian://`
URL scheme when opening a note, is derived from this path's last component,
so the folder name must match the vault's name as shown in Obsidian.

`debug` controls whether every incoming message is appended to
`host.log` (see below). It's off by default — every page visit sends a
message, and the log is never trimmed or rotated, so leaving it on
indefinitely just grows the file unboundedly. Turn it on temporarily
when you actually need to watch what's happening.

## Note path scheme

A note's path is `<folder>/<domain>/<normalized-path>.md`, e.g.
`https://github.com/owner/repo/issues/123` becomes
`webnotes/github.com/owner__repo__issues__123.md`. Only `https://` URLs are
supported. The path is normalized by percent-decoding each segment (for
readability) and joining them with `__` (since a literal `/` can't be part
of a single file name); an empty path becomes `index`. The query string and
fragment aren't part of the path, so e.g. `?tab=readme` and `?tab=files` on
the same page collapse to the same note.

## Trying it out

Set `debug = true` in `config.toml` first (see above), then:

```bash
tail -f ~/.local/share/skagedal-tools/chrome-page-notes/host.log
```

Then click the extension's toolbar icon, or navigate to a new page in any
tab — a new line, including the URL, should appear in the log each time.

## Icon

`extension/icons/icon.svg` is the source; `extension/icons/icon{16,32,48,128}.png`
are rasterized from it (`convert -background none icon.svg -resize <N>x<N> icon<N>.png`,
from ImageMagick) and are what's actually referenced in `manifest.json`.
Regenerate them if the SVG changes.
