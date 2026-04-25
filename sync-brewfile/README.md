# sync-brewfile

Reconcile locally installed Homebrew packages against a Brewfile.

For every formula installed on request (`brew leaves --installed-on-request`)
and every cask (`brew list --cask`) that is not declared in your Brewfile, you
are prompted with three options:

1. **Add to Brewfile** — append a `brew "name"` or `cask "name"` line.
2. **Uninstall** — run `brew uninstall [--cask] <name>`.
3. **Keep evaluating** — do nothing and move on.

## Usage

```sh
sync-brewfile ~/code/dotfiles/Brewfile
```

## Notes

- Only top-level formulae are considered; transitive dependencies are ignored
  because they shouldn't be in the Brewfile anyway.
- Brewfile parsing is line-based and recognises `brew "..."` and `cask "..."`
  with optional trailing options (`brew "git", restart_service: :changed`).
  Other directives (`tap`, `mas`, `vscode`, …) are left alone.
- New entries are appended to the end of the Brewfile. Re-organise by hand if
  you want them grouped.
