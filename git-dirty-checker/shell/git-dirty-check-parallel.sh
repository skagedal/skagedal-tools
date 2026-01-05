#!/usr/bin/env bash
# git-dirty-check-parallel.sh
# Find git repositories with uncommitted changes (parallel version using GNU parallel)
#
# Usage: git-dirty-check-parallel.sh /path/to/repos [/another/path ...]
#
# This will list all subdirectories that are git repositories with uncommitted changes.
# Requires GNU parallel to be installed.

printf '%s\n' "$@"/*/ | parallel 'git -C {} status --porcelain 2>/dev/null | grep -q . && realpath {}'
