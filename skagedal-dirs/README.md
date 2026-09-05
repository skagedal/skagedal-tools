# skagedal-dirs

The XDG base directories the tools in this repo keep their files in. Not a
tool — a library crate the other crates depend on, so the layout is defined
in exactly one place.

```rust
skagedal_dirs::config_dir("tracker")  // ~/.config/skagedal-tools/tracker
skagedal_dirs::data_dir("tracker")    // ~/.local/share/skagedal-tools/tracker
skagedal_dirs::cache_dir("tracker")   // ~/.cache/skagedal-tools/tracker
```

The XDG layout is used on macOS too, rather than
`~/Library/Application Support/tech.skagedal.<tool>/`: having the same paths
on every machine matters more here than following Apple's convention.

`$XDG_CONFIG_HOME`, `$XDG_DATA_HOME` and `$XDG_CACHE_HOME` override the
respective roots when set to an absolute path, per the [XDG base directory
specification](https://specifications.freedesktop.org/basedir-spec/latest/).
A relative or empty value is ignored, as the spec requires. Tests use these
to point a tool at a temporary directory.

Tools written in other languages compose the same paths themselves; see
`log-viewer/browser/src/config.ts`.

`scripts/one-offs/migrate-xdg-paths` did the one-shot move off the older
`~/.skagedal-tools/` layout.
