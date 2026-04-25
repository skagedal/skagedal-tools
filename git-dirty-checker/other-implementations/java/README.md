# git-dirty-checker-java

A tool to find git repositories with uncommitted changes (dirty working trees) in subdirectories of specified paths.

## Usage

```bash
java -jar target/git-dirty-checker-java.jar /path/to/repos /another/path
```

This will list all subdirectories that are git repositories with uncommitted changes.

## Alternatives

This functionality can also be achieved with simple shell one-liners:

### Simple version (sequential)

```bash
for dir in "$@"/*/; do git -C "$dir" status --porcelain 2>/dev/null | grep -q . && realpath "$dir"; done
```

### Parallel version (using GNU parallel)

```bash
printf '%s\n' "$@"/*/ | parallel 'git -C {} status --porcelain 2>/dev/null | grep -q . && realpath {}'
```

Both one-liners output the full paths of repositories with uncommitted changes, one per line.
