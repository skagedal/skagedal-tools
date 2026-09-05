# chrome-page-notes

A Chrome extension for attaching your own notes to a page.

## Purpose

Attaches your own notes to the pages you visit, stored as plain markdown
files in an Obsidian vault:

- Clicking the toolbar icon opens a popup for the current tab. If a note
  already exists for that URL, its content is shown inline (as plain text,
  not rendered) with an "Open in Obsidian" button — which also brings the
  Obsidian app to the foreground. Otherwise, a "Create note" button creates
  one.
- The toolbar icon shows a ✓ badge whenever the current tab has a note, kept
  up to date by a background service worker as you navigate or switch tabs.
  If a request fails, it shows a "!" badge instead, and the popup offers an
  "Open Obsidian" button.

Chrome's extension APIs have no direct file system access, so this is all
done from the native messaging host described below. Looking up and
creating notes reads and writes files directly in the vault folder on
disk — no dependency on the `obsidian` CLI or the app being open at all.
The one exception is "Open in Obsidian", which does need the app: it goes
through the `obsidian://open?vault=...&file=...` URL scheme (handled by the
app itself), which launches the app if needed and reliably lands on the
right vault and note.

(An earlier version shelled out to the [`obsidian`
CLI](https://obsidian.md/plugins?id=cli) for everything, but its `vault=`
targeting option turned out to be silently ignored — every command actually
operated on whichever vault window happened to be focused, which both
under-reported notes and once created a note in the wrong vault entirely.)

The extension's `manifest.json` has a fixed `"key"` field, which pins its
extension ID to `jbgofjilflakfjbenbgpppajapiffphn` no matter where it's
loaded unpacked from — that ID is also hardcoded in `register-host.sh`, so
the two stay in sync without per-machine edits.

## Loading the extension

1. Go to `chrome://extensions`
2. Enable "Developer mode" (top right)
3. Click "Load unpacked" and select this directory
4. Click the extension's icon in the toolbar — a popup should show whether the current page has a note

## Reloading after changes

Click the reload icon for the extension on `chrome://extensions`. Since the
`tabs` permission and the background service worker were added after the
first load, Chrome will show a "new permissions" prompt on that reload —
click "Update extension" to accept it, or the URL-reporting features won't
be active.

## Native messaging host

`host/` is a small Rust program (`chrome-page-notes-host`, a member of this
repo's Cargo workspace) that the browser extension talks to over stdin/stdout
using Chrome's [native messaging
protocol](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging).
It reads and writes note files directly in the configured vault folder, and
opens notes in Obsidian (installed e.g. via `brew install --cask obsidian`)
through its `obsidian://` URL scheme.

Register it with Chrome (builds the release binary and writes the host
manifest to `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/`):

```bash
./register-host.sh
```

Re-run this after updating the host code, or if the repo moves (the host
manifest embeds an absolute path to the built binary).

### Configuration

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

### Note path scheme

A note's path is `<folder>/<domain>/<normalized-path>.md`, e.g.
`https://github.com/owner/repo/issues/123` becomes
`webnotes/github.com/owner__repo__issues__123.md`. Only `https://` URLs are
supported. The path is normalized by percent-decoding each segment (for
readability) and joining them with `__` (since a literal `/` can't be part
of a single file name); an empty path becomes `index`. The query string and
fragment aren't part of the path, so e.g. `?tab=readme` and `?tab=files` on
the same page collapse to the same note.

### Trying it out

Set `debug = true` in `config.toml` first (see above), then:

```bash
tail -f ~/.local/share/skagedal-tools/chrome-page-notes/host.log
```

Then click the extension's toolbar icon, or navigate to a new page in any
tab — a new line, including the URL, should appear in the log each time.

## Icon

`icons/icon.svg` is the source; `icons/icon{16,32,48,128}.png` are rasterized
from it (`convert -background none icon.svg -resize <N>x<N> icon<N>.png`,
from ImageMagick) and are what's actually referenced in `manifest.json`.
Regenerate them if the SVG changes.
