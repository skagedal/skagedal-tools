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

- Use **pnpm** as the package manager
- The package manifest is `package.json5`, not `package.json`. pnpm reads
  `package.json5` natively. The file should start with a
  `// Manifest for <tool-name>` comment and use unquoted top-level keys
  (`name`, `version`, `scripts`, …). Nested keys stay quoted (many aren't
  valid JS identifiers). No `package.json` is committed or generated — modern
  Node (≥ 23) auto-detects ESM from `import`/`export` syntax, and tsx has its
  own ESM detection, so the manifest stub isn't needed.
- Every tool must have a `test` script. If there's nothing meaningful to test
  yet, use `tsc --noEmit` as a baseline so `./install --check` exercises the
  type-checker.
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

## Rust Projects

All Rust projects in this repository must follow these guidelines:

- Use `edition = "2024"` in Cargo.toml
  - This ensures projects use the latest Rust edition with modern language features and best practices
