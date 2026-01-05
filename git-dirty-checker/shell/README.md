# Shell Scripts for git-dirty-checker

Simple shell script implementations to find git repositories with uncommitted changes.

## Scripts

### git-dirty-check-serial.sh

Sequential version that checks repositories one by one.

**Usage:**
```bash
./git-dirty-check-serial.sh /path/to/repos /another/path
```

### git-dirty-check-parallel.sh

Parallel version using GNU parallel for faster checking of many repositories.

**Usage:**
```bash
./git-dirty-check-parallel.sh /path/to/repos /another/path
```

**Requirements:** GNU parallel must be installed.

## Output

Both scripts output the full paths of repositories with uncommitted changes, one per line.
