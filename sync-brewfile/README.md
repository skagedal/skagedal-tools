# sync-brewfile

Reconcile locally installed Homebrew packages against a Brewfile.

For every formula installed on request (`brew leaves --installed-on-request`)
and every cask (`brew list --cask`) that is not declared in your Brewfile, the
package's name (linked to its homepage) and description are shown along with
an interactive menu:

- **Add to Brewfile** — append a `brew "name"` or `cask "name"` line.
- **Uninstall** — run `brew uninstall [--cask] <name>`.
- **Skip** — do nothing and move on.
- **Quit** — exit `sync-brewfile`.

### Keys

- `j` / `k` (or arrow keys) — move the highlight up/down.
- `Enter` — confirm the highlighted option.
- `a` / `d` / `s` / `q` — pick Add / unInstall (Delete) / Skip / Quit directly.
- `Esc` / `Ctrl+C` — also quit.

## Usage

```sh
sync-brewfile path/to/Brewfile          # interactive mode
sync-brewfile --list path/to/Brewfile   # just print the extras and exit
```

## Notes

- Only top-level formulae are considered; transitive dependencies are ignored
  because they shouldn't be in the Brewfile anyway.
- Brewfile parsing is line-based and recognises `brew "..."` and `cask "..."`
  with optional trailing options (`brew "git", restart_service: :changed`).
  Other directives (`tap`, `mas`, `vscode`, …) are left alone.
- New entries are appended to the end of the Brewfile. Re-organise by hand if
  you want them grouped.
