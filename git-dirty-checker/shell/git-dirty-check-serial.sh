#!/bin/bash
# git-dirty-check-serial.sh
# Find git repositories with uncommitted changes (sequential version)
#
# Usage: git-dirty-check-serial.sh /path/to/repos [/another/path ...]
#
# This will list all subdirectories that are git repositories with uncommitted changes.

for dir in "$@"/*/; do
    git -C "$dir" status --porcelain 2>/dev/null | grep -q . && realpath "$dir"
done
