# TypeScript CLI Argument Parsing Libraries

A side-by-side comparison of popular CLI argument parsing libraries for Node.js/TypeScript. Each library implements the same demo program so the differences in API style, type safety, and features are easy to compare.

## The demo CLI

Each file under `src/` implements a `greet-demo` program with two subcommands:

```
greet-demo greet <name> [--times <n>] [--shout]
greet-demo farewell <name> [--formal]
```

| Argument / option | Type | Default | Description |
|---|---|---|---|
| `greet <name>` | positional, required | — | Name to greet |
| `--times, -t` | number | `1` | How many times to repeat |
| `--shout, -s` | boolean | `false` | Output in uppercase |
| `farewell <name>` | positional, required | — | Name to address |
| `--formal, -f` | boolean | `false` | Use formal language |

## Running the examples

```bash
pnpm install

pnpm run commander -- greet Alice --times 3 --shout
pnpm run yargs    -- farewell Bob --formal
pnpm run meow     -- greet Alice --times 2
pnpm run citty    -- farewell Bob
pnpm run gunshi   -- greet Alice --times 2
```

## Comparison

| Feature | [commander](https://github.com/tj/commander.js) | [yargs](https://yargs.js.org) | [meow](https://github.com/sindresorhus/meow) | [citty](https://github.com/unjs/citty) | [gunshi](https://github.com/kazupon/gunshi) |
|---|---|---|---|---|---|
| **Package size** (self, excl. deps) | 198 KB | 337 KB | 420 KB | 48 KB | 211 KB |
| **Direct dependencies** | 0 | 7 | 0 | 1 (`consola`) | 0 (bundled) |
| **TypeScript** | Bundled types | `@types/yargs` | Bundled types | Written in TS, inferred arg types | Written in TS, inferred arg types |
| **ESM** | ✓ | ✓ | ✓ (only) | ✓ | ✓ |
| **CJS** | ✓ | ✓ | ✗ | ✓ | ✓ |
| **Subcommands** | Native | Native | Manual dispatch | Native | Native |
| **Nested subcommands** | ✓ | ✓ | Manual | ✓ | ✓ |
| **Positional arguments** | `<required>` / `[optional]` in signature | `.positional()` builder | `cli.input` array | `type: 'positional'` | `type: 'positional'` |
| **Option types** | string, boolean, int, float, custom | string, boolean, number, array, count | string, boolean, number, string\[\] | string, boolean | string, boolean, number, positional |
| **Required options** | ✓ | ✓ | ✓ (`isRequired`) | ✓ | ✓ |
| **Default values** | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Auto-generated help** | ✓ | ✓ | ✓ (free-form text) | ✓ | ✓ |
| **Version flag** | ✓ | ✓ | ✓ (from `package.json`) | ✓ | ✓ |
| **Async action handlers** | ✓ (`.parseAsync()`) | ✓ | N/A | ✓ | ✓ |
| **Shell completion** | ✗ | ✓ bash/zsh/fish | ✗ | ✗ | ✓ bash/zsh/fish/powershell (plugin) |
| **Node.js requirement** | ≥ 18 | ≥ 12 | ≥ 18 | ≥ 18 | ≥ 20 |

### Notes

**commander** — The most widely used CLI library. Zero dependencies, ships dual ESM/CJS, bundled types. Subcommand syntax uses a natural string signature (`"greet <name>"`). No built-in shell completion; third-party packages like [`omelette`](https://github.com/arturadib/omelette) can add it.

**yargs** — Feature-rich with a fluent builder API. The only library here with built-in shell completion (bash/zsh/fish). Has more option types than the others (arrays, count). The `@types/yargs` package provides TypeScript types; inference from builder calls is decent but not as tight as citty.

**meow** — Minimal and intentionally simple; designed for single-command CLIs. Subcommands require manual dispatch (inspect `cli.input[0]` and branch). ESM-only. Suitable for small tools where you want total control with minimal abstraction.

**citty** — Modern, lightweight (smallest package), written in TypeScript. Offers the best type inference: arg shapes defined in `args` flow through automatically to the `run` function, with no need for manual type annotations. Subcommands declared as plain objects via `subCommands`. CJS and ESM.

**gunshi** — A newer, declarative library with a `define()` + `cli()` API. Like citty, it is written in TypeScript and infers arg types directly from the definition. Supports named positional arguments (`type: 'positional'`) alongside regular options. Has an extensible plugin system; shell completion (bash/zsh/fish/PowerShell) is provided by `@gunshi/plugin-completion`, passed as a plugin to `cli()`. Requires Node.js ≥ 20.

## Shell completions

Only **yargs** has built-in shell completion support. The `.completion()` call adds a `completion` subcommand that outputs a bash/zsh completion script:

```bash
# Print the completion script
pnpm --silent run yargs -- completion

# Activate completions in the current shell session
source <(pnpm --silent run yargs -- completion)

# Install permanently (bash)
pnpm --silent run yargs -- completion >> ~/.bash_completion

# Install permanently (zsh — add before compinit)
pnpm --silent run yargs -- completion >> ~/.zshrc
```

The generated script calls the program with `--get-yargs-completions` to dynamically suggest completions for commands and options.

**gunshi** also has completion support, via the `@gunshi/plugin-completion` plugin (bash, zsh, fish, PowerShell):

```bash
# Print the completion script
pnpm --silent run gunshi -- complete bash

# Activate completions in the current shell session
source <(pnpm --silent run gunshi -- complete bash)
```
