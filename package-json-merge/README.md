# package-json-merge

A git merge driver for `package.json` that resolves the most common conflict
automatically: when two branches have bumped the same dependency to different
versions, the higher semver range wins.

Other types of conflicts (different `scripts`, structural disagreements, ranges
that aren't semver like `workspace:*` or `github:owner/repo`) fall through to
`git`'s default merge behavior, leaving conflict markers for you to resolve.

## What it does

For `dependencies`, `devDependencies`, `peerDependencies`, and
`optionalDependencies`:

- If only one side changed a dep, take that side.
- If both sides set a dep to the same value, take it.
- If both sides bumped the same dep to different valid semver ranges, take the
  one with the higher minimum version.
- If one side removed a dep and the other modified it, that's a conflict.
- If either side's range isn't a parseable semver range, that's a conflict.

For everything else (top-level keys like `scripts`, `version`, etc.):

- Standard 3-way: if only one side changed, take it; if both made the same
  change, take it; otherwise it's a conflict.

When the merge would be conflicted, the tool falls back to `git merge-file` so
you get the usual conflict markers in the file.

The lockfile (`pnpm-lock.yaml`, `package-lock.json`) is **not** touched — let
your package manager handle that. For pnpm, see
[`@pnpm/merge-driver`](https://github.com/pnpm/merge-driver).

## Install

Wire it up for the current repo (writes to `.git/config` and
`.git/info/attributes`, so nothing is committed):

```sh
package-json-merge install
```

Or globally (writes to `~/.gitconfig` and the global git attributes file):

```sh
package-json-merge install --global
```

To remove the wiring:

```sh
package-json-merge uninstall          # this repo
package-json-merge uninstall --global # global
```

## How git invokes it

The `install` command registers a driver named `skagedal-package-json` that
runs:

```
package-json-merge merge %O %A %B %P
```

where `%O` is the ancestor, `%A` the current branch's file (written in place),
`%B` the other branch's file, and `%P` the original pathname.

## Trying it out

A small two-repo setup is enough to see the driver fire on a real merge:

```sh
# 1. Create the upstream repo with one harmless dep.
mkdir first && cd first
git init -q
cat > package.json <<'EOF'
{
  "name": "merge-driver-demo",
  "version": "1.0.0",
  "dependencies": {
    "lodash": "^4.17.0"
  }
}
EOF
git add package.json && git commit -q -m "initial"

# 2. Clone it.
cd ..
git clone -q first second

# 3. Bump the dep to different versions in each clone.
cd first
sed -i '' 's/\^4.17.0/^4.17.21/' package.json
git commit -q -am "bump lodash to ^4.17.21"

cd ../second
package-json-merge install   # wire the driver up in this clone
sed -i '' 's/\^4.17.0/^4.17.15/' package.json
git commit -q -am "bump lodash to ^4.17.15"

# 4. Pull and watch the driver pick the higher range.
git pull --no-rebase --no-edit origin main
cat package.json   # lodash should be ^4.17.21
```

Without the driver, step 4 leaves conflict markers around the `lodash` line. With
it installed in `second`, the merge completes cleanly and the higher range wins.

To compare against the fallback behavior, run `package-json-merge uninstall` in
`second` and redo the experiment with a fresh pair of conflicting bumps.

## Limitations

- Only operates on files matched by `package.json` in `.gitattributes`. If you
  use `package.json5` or another manifest format, you can add it manually.
- Doesn't try to merge inside `scripts`, `engines`, or other non-dep objects
  beyond a basic 3-way comparison.
- "Higher minVersion wins" is a heuristic. It can pull in a major bump that one
  branch was deliberately holding back. Check the diff after a merge if that
  matters for you.
