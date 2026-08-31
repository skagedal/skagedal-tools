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
files, caches, snooze records, etc.), put it under:

```
~/.skagedal-tools/<tool-name>/
```

For example, `git-dirty-checker` stores its snooze records in
`~/.skagedal-tools/git-dirty-checker/snoozed/`, and `cloudwatch-insights`
reads `~/.skagedal-tools/cloudwatch-insights/settings.toml`.

Do not use `~/.config/<tool>/` or `$XDG_CONFIG_HOME` — those aren't
meaningfully portable to macOS (the OS convention there is
`~/Library/Application Support/`), and keeping everything under
`~/.skagedal-tools/` makes it easy to back up, wipe, or sync as a group.

Tools may honor a `SKAGEDAL_TOOLS_HOME` environment variable to
override the base directory (primarily useful for tests).

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

## Rust Projects

All Rust projects in this repository must follow these guidelines:

- Use `edition = "2024"` in Cargo.toml
  - This ensures projects use the latest Rust edition with modern language features and best practices
- Code must be clippy-clean. The top-level `./check` runs
  `cargo clippy --all-targets -- -D warnings && cargo test` per crate, so any
  clippy lint becomes a CI failure. Fix the lint at the source rather than
  reaching for `#[allow(...)]`.
