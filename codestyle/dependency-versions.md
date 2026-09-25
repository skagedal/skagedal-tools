# Dependency versions in JavaScript manifests

Every dependency in a `package.json5` (or `package.yaml`) is written as
`"latest"`:

```json5
  dependencies: {
    "gunshi": "latest",
    "react": "latest"
  },
```

Not `^1.2.3`, not `~1.2`, not an exact version. The range is not where
the decision lives.

## Why

**The lock file is the pin.** `pnpm-lock.yaml` is committed, and it names
the exact version of every package, direct and transitive. That is what a
fresh `pnpm install` gets, what CI gets, and what a build reproduces from.
A caret in the manifest changes none of that — it only narrows what a
future `pnpm update` is allowed to pick.

**`minimumReleaseAge` is the safety margin.** `pnpm-workspace.yaml` sets
`minimumReleaseAge: 4320` — three days — so no version published in the
last three days is installed at all, by `pnpm update` or by a plain
`pnpm install`. The thing a hand-written range is usually reached for is
protection against a bad or hostile release, and a release-age floor is a
far better answer to that than a major-version bound: a compromised
`1.2.4` satisfies `^1.2.3` perfectly well.

**Ranges written by hand drift into lies.** `"^19.1.0"` was true when
somebody typed it and the lock file said 19.1.0. Two updates later the
lock file says 19.2.8 and the manifest still says 19.1.0 — a floor nobody
chose, describing a version nobody is running. Nothing enforces it,
nothing re-reads it, and it quietly stops meaning anything. `"latest"`
cannot go stale, because it makes no claim about a version.

**A major bound is not a review.** A caret stops `pnpm update` at the
major boundary, which feels like a gate but isn't one: it blocks the
update without telling anyone why, and the version sits there unnoticed
until something else forces the question. Crossing a major deserves
attention when the code breaks, which is what the check suite is for —
not a line in a manifest read once a year.

## How updating works

```
pnpm update --no-save
```

`--no-save` because there is nothing to save: the manifest already asks
for the newest, so the whole result of the update is a rewritten lock
file. Majors are crossed freely. Then run the tool's `check` script, read
the lock file diff, and commit both together — the diff is the changelog,
and a build that stops working is the signal that a major actually
mattered.

Repos with several ecosystems usually have a `ci/update-dependencies`
script that does this alongside `cargo update`, `pub upgrade` and the
rest. A weekly workflow, `.github/workflows/update-dependencies.yml`,
runs it and opens a pull request when something moved, and CI on that
pull request is the check suite doing its job.

## When to deviate

Pin a range when a dependency is genuinely broken at its newest version
and the fix is not yours to make. Write the range, and write a comment
next to it saying what is wrong and what would let it be removed:

```json5
  devDependencies: {
    // Pinned to ^9: eslint-plugin-react does not yet support eslint v10.
    // See https://github.com/jsx-eslint/eslint-plugin-react/issues/3977
    "@eslint/js": "^9",
    "eslint": "^9",
  }
```

A pin without that comment is indistinguishable from one that was left
behind, which is exactly the state this convention exists to avoid.

The `packageManager` field is a separate thing and stays exactly pinned —
`"pnpm@10.33.0"`. It says which pnpm runs rather than what pnpm installs,
and corepack verifies it.

## Other ecosystems

This is about JavaScript, where a committed lock file and a release-age
floor together make the range redundant. Cargo and pub keep hand-written
ranges in `Cargo.toml` and `pubspec.yaml`: neither has an equivalent of
`minimumReleaseAge`, so there the bound is still doing work.
