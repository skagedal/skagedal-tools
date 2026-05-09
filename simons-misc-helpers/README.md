# simons-misc-helpers

While this repo (skagedal-tools) is a collection of tools that do not (yet) deserve their own git repository, this tool is a collection of even smaller helpers that do not even deserve their own tool. 

Think of it as "this could have been a bash script". (But they never stay bash-script-sized, do they?)

##  Subcommands

### format-dependency-changes

This is a helper for Andrew Nesbitt's [git-pkgs](https://github.com/git-pkgs/git-pkgs) tool. it takes the output of `git pkgs diff --format=json` and formats it again, similar to the output of `git pkgs diff`, but with a difference that I want – major version bumps are highlighted.

Example, taken from running `./update` in this very repo (skagedal-tools), which bumps dependencies across both Rust crates and pnpm workspaces:

```
Modified (major version updates):
  ~ regex 0.2.11 -> 1.12.3 (assistant/Cargo.lock)
  ~ tsx 3.14.0 -> 4.21.0 (linear-notifications/pnpm-lock.yaml)

Modified (minor version updates):
  ~ serde 1.0.219 -> 1.0.228 (assistant/Cargo.lock)
```

In a real terminal, section headers are bold, the `~` markers and package names are yellow, the manifest paths and the from-version are dimmed, and under "Modified (major version updates):" the new version number stands out in red. Sections for `Added:` and `Removed:` work analogously and only appear when there's something to show.
