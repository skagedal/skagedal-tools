# intellij-patch

Apply XML patches to IntelliJ project files (`.idea/*.xml`) so that a fresh
clone or worktree picks up your usual configuration without clicking through
the IDE.

The tool is generic: it reads a TOML config that describes which files to
patch and what XML elements to ensure are present.

## Usage

```sh
intellij-patch                        # apply patches to the current directory
intellij-patch /path/to/project       # apply patches to a specific project
intellij-patch --dry-run              # show what would change
intellij-patch --config some.toml     # override the config path
```

By default the config is read from
`~/.skagedal-tools/intellij-patch/config.toml` (override with
`SKAGEDAL_TOOLS_HOME` or `--config`).

Each run is idempotent — already-correct files are reported as `ok`; missing
elements are added; the file is written back only when the resulting XML
differs from what was on disk.

## Config format

Each `[[patch]]` describes one file in the project tree.

```toml
[[patch]]
name = "example-patch"
file = ".idea/example.xml"

# Optional: only apply when the project is under one of these directories.
# Tilde is expanded. Omit (or set to []) to apply to any project.
applies-when-under = ["~/projects", "~/worktrees"]

# Optional: if the file does not exist, start from this template.
# Without this, missing files are skipped.
create-if-missing = """<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
</project>
"""

# One or more "ensure-children" rules per patch.
[[patch.ensure-children]]
parent = "project/component[@name='ExampleSettings']/option[@name='entries']/list"
match-by = "@value"
fragments = [
    '<Entry value="first"/>',
    '<Entry value="second"/>',
]
```

### `parent`

A `/`-separated path of element steps. Each step is either `tag` or
`tag[@attr='value']` (multiple `[...]` predicates allowed). The first step
must equal the document root.

If a step is missing along the way, an empty element is created with the
attributes from its predicates. So a path like
`project/component[@name='Foo']/list` will, on a brand-new file, create
`<component name="Foo"><list/></component>` under the existing `<project>`.

### `match-by`

An expression evaluated against each existing child of the parent (and against
each fragment) to decide whether a fragment is already present. Supported
forms:

- `@attr` — the element's own attribute
- `tag/@attr` — an attribute on a (named) descendant
- `tag/tag/@attr` — deeper descendant attribute

Fragments whose `match-by` value already appears among the existing children
are skipped; otherwise they are appended.

### `fragments`

An array of XML strings. Each must be a single well-formed element.

## Notes

- Files are written with a `<?xml version="1.0" encoding="UTF-8"?>`
  declaration, two-space indentation, and `<elem ... />` self-closing style —
  matching the format IntelliJ writes itself.
- Attribute order in the output follows the order of the input fragments.
- Comments and CDATA in the original file are preserved by the underlying
  XML library, but unusual whitespace will be normalised. IntelliJ rewrites
  these files itself anyway.
