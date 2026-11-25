# Contributing

Where the conventions of this repository are written down:

- [AGENTS.md](AGENTS.md) — what to do inside `skagedal-tools`, per
  language: Rust, Swift, Node.js and shell scripts, where tools keep
  their config and state, and keeping the README's lists current. It is
  written for coding agents, and it is the rulebook for everyone else too.
- [codestyle/](codestyle/) — conventions that apply across repos:
  - [react.md](codestyle/react.md) — React apps: effects in hooks,
    react-query for the server, Temporal for dates, the ESLint setup.
  - [dependency-versions.md](codestyle/dependency-versions.md) — why
    JavaScript manifests write `"latest"`.
- [specs/](specs/README.md) — when a change needs a written spec, and how
  one is drafted and numbered.

Before sending a change, run `./check` (or `./check <tool>`), which is
what CI runs: formatting, lints and tests for every language.
