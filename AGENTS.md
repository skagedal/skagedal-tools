# Instructions for AI Agents

When working on this repository, please follow these guidelines:

## Tools Table Maintenance

The README.md file contains a table listing all tools in the repository. **This table must be kept up-to-date** whenever:

- A new tool is added to the repository
- A tool is removed from the repository
- A tool's description changes significantly

The table format is:

```markdown
| Tool | Description |
|------|-------------|
| [tool-name](tool-name/) | Brief description of the tool |
```

Each tool name should be a link to its directory, and the description should be concise (one line).

## Java Code Formatting

All Java code in this repository must be formatted with **4-space indentation**. This applies to:

- Class and method bodies
- Control structures (if, for, while, etc.)
- Method chaining and fluent APIs
- All other indented code blocks

Do not use tabs for indentation in Java files.

## Shell Scripts

All shell scripts in this repository must follow these guidelines:

- Use `#!/usr/bin/env bash` as the shebang line instead of `#!/bin/bash`
  - This provides better portability across different systems where bash may be installed in different locations
- Use dashes (not underscores) in bash function names (e.g. `update-cargo`, not `update_cargo`)
- Stay compatible with **bash 3.2**, which is what macOS ships by default.
  Avoid features introduced in later versions, including:
  - `printf '%(...)T'` (bash 4.2+) — synthesise timestamps from arithmetic, or
    shell out to `date`
  - associative arrays / `declare -A` (bash 4.0+)
  - `${var,,}` / `${var^^}` case-conversion expansions (bash 4.0+)
  - `mapfile` / `readarray` (bash 4.0+)
  - `&>>` append redirection (bash 4.0+; use `>>file 2>&1` instead)

## Node.js Programs

All Node.js/TypeScript projects in this repository must follow these guidelines:

- Use **pnpm** as the package manager. Use `pnpm dlx <pkg>` for one-off
  invocations (do not use `npx`).
- The package manifest is `package.json5`, not `package.json`. pnpm reads
  `package.json5` natively. The file should start with a
  `// Manifest for <tool-name>` comment and use unquoted top-level keys
  (`name`, `version`, `scripts`, …). Nested keys stay quoted (many aren't
  valid JS identifiers). No `package.json` is committed or generated — modern
  Node (≥ 23) auto-detects ESM from `import`/`export` syntax, and tsx has its
  own ESM detection, so the manifest stub isn't needed.
- Every tool must have a `check` script. At a minimum it runs `tsc --noEmit`.
  If the tool has a test suite, add a `test` script (e.g. `tsx --test test/*.test.ts`)
  and have `check` invoke it (`"check": "tsc --noEmit && pnpm test"`).
  If the tool uses React, also add a `lint` script that runs `eslint .` and
  invoke it from `check` too. The top-level `./check` runs `pnpm run check`
  in every Node tool, so this is the entry point CI exercises.
- React tools should use ESLint v9 with the flat-config (`eslint.config.js`)
  setup, including `@eslint/js`, `typescript-eslint`, `eslint-plugin-react`,
  and `eslint-plugin-react-hooks`. See `log-viewer/eslint.config.js` for a
  reference. Pin `eslint` to `^9.x` until `eslint-plugin-react` supports v10.
- Set `minimumReleaseAge: 4320` in `pnpm-workspace.yaml` (4320 minutes = 3 days):
  ```yaml
  minimumReleaseAge: 4320
  ```
  This is a pnpm supply-chain security feature that prevents installing package versions
  published fewer than 3 days ago, giving the community time to detect and pull compromised releases.
- Use **[gunshi](https://github.com/kazupon/gunshi)** for CLI argument parsing. It is
  declarative (`define({ args, run })` + `cli()`), written in TypeScript with inferred
  argument types, and supports shell completion via `@gunshi/plugin-completion`. Prefer
  it over commander/yargs/meow/citty for new tools. See `comparison-typescript-cli-arguments/src/gunshi.ts`
  for a reference setup, including subcommands and completion.
  - gunshi is ESM-first and requires Node.js ≥ 20. Use `"module": "ESNext"` +
    `"moduleResolution": "Bundler"` in `tsconfig.json` so tsc emits ESM unconditionally
    without consulting `package.json`. Node ≥ 23 detects ESM from `import`/`export`
    syntax at runtime, so no `"type": "module"` declaration is needed.
  - Pass `renderHeader: null` to `cli()` when you don't want the command description
    reprinted before every invocation.

## Per-tool Config and State Directories

When a tool needs to store user-scoped config or state on disk (config
files, caches, snooze records, etc.), put it under the XDG base
directories, namespaced by `skagedal-tools/<tool-name>`:

```
~/.config/skagedal-tools/<tool-name>/       config
~/.local/share/skagedal-tools/<tool-name>/  data and state
~/.cache/skagedal-tools/<tool-name>/        throwaway caches
```

For example, `git-dirty-checker` stores its snooze records in
`~/.local/share/skagedal-tools/git-dirty-checker/snoozed/`, and
`cloudwatch-insights` reads
`~/.config/skagedal-tools/cloudwatch-insights/settings.toml`.

This is the XDG layout on macOS too, rather than Apple's
`~/Library/Application Support/` convention: having the same paths on every
machine matters more here than following the platform, and the reverse-DNS
directory names the `directories` crate produces
(`~/Library/Application Support/tech.skagedal.tracker/`) are a mouthful.

Rust crates get these paths from the workspace's `skagedal-dirs` crate —
`skagedal_dirs::{config_dir, data_dir, cache_dir}("<tool-name>")` — rather
than composing them by hand. Tools in other languages compose the same
paths themselves; see `log-viewer/browser/src/config.ts`.

The base directories honor `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME` and
`$XDG_CACHE_HOME` when those hold an absolute path, per the XDG base
directory specification. That is also how tests point a tool at a temporary
directory. (The older single-root `SKAGEDAL_TOOLS_HOME` override is gone —
it doesn't map onto three separate roots.)

## Flutter Projects

All Flutter/Dart projects in this repository must follow these guidelines:

- The Flutter SDK is pinned per app with [fvm](https://fvm.app): the version
  lives in the app's `.fvmrc`, which is committed, while the `.fvm/` directory
  of machine-local symlinks is not. Run Flutter through `fvm flutter ...`, not
  a `flutter` off `$PATH`. Change the version with `fvm use <version>` and
  commit the resulting `.fvmrc`; CI reads it via `flutter-version-file` and so
  needs no separate bump.
- Keep `flutter analyze` clean. The top-level `./check` runs
  `flutter pub get && flutter analyze && flutter test` per app, so any lint
  becomes a CI failure. Fix the lint rather than adding `// ignore:`.
- Keep the platform-independent logic — timing, state machines, formatting —
  in plain Dart classes that do not import plugins, and have widgets and
  controllers take their platform dependencies by constructor injection.
  Plugin calls can't run under `flutter test`, so anything reached through a
  static plugin call is untestable.
- Generated binary assets (sounds, images) should ship with the script that
  generated them, under the app's `tool/` directory.

## Swift Projects

All Swift projects in this repository must follow these guidelines:

- Use `// swift-tools-version:6.2` and `swiftLanguageModes: [.v6]` in
  `Package.swift`, so packages build under strict concurrency checking.
- Formatting and linting go through `swift format`, which ships with the
  toolchain. Each package has a `.swift-format` config, and the top-level
  `./check` runs `swift format lint --strict` before building, so any finding
  becomes a CI failure. Run `swift format --in-place --recursive` before
  committing. Do not add SwiftLint as a package dependency — that makes
  `swift build` compile a linter.
- Use **swift-testing** (`import Testing`, `@Suite`, `@Test`, `#expect`) for new
  tests, not XCTest.
- Use **[swift-argument-parser](https://github.com/apple/swift-argument-parser)**
  for CLI argument parsing rather than reading `CommandLine.arguments` by hand.
- macOS-only packages (anything importing AppKit) can't be checked on the
  Linux runner the rest of `./check` uses. They go in `SWIFT_TOOLS` in
  `shared.sh`, where `check-swift` skips them off macOS, and get exercised by
  the separate `check-swift` job in `.github/workflows/tests.yml`.
- `install-swift` copies the release binary to `~/.local/bin`, since SwiftPM
  has no equivalent of `cargo install`.

## Rust Projects

All Rust projects in this repository must follow these guidelines:

- Use `edition = "2024"` in Cargo.toml
  - This ensures projects use the latest Rust edition with modern language features and best practices
- Code must be clippy-clean. The top-level `./check` runs
  `cargo clippy --all-targets -- -D warnings && cargo test` per crate, so any
  clippy lint becomes a CI failure. Fix the lint at the source rather than
  reaching for `#[allow(...)]`.
- Code must be rustfmt-clean. `./check` also runs `cargo fmt --all --check`, so
  unformatted code is a CI failure. Run `cargo fmt` before committing. Don't
  hand-format around rustfmt or scatter `#[rustfmt::skip]`.
