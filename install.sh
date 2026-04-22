#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

install-node() {
    local dir="$1"
    echo "==> Installing $dir"
    (cd "$SCRIPT_DIR/$dir" && pnpm link --global)
}

install-rust() {
    local dir="$1"
    echo "==> Installing $dir"
    (cd "$SCRIPT_DIR/$dir" && cargo install --path .)
}

install-node linear-notifications
install-node cloudwatch-insights

install-rust git-dirty-checker
install-rust log-jsonify
install-rust x-java-home

echo "==> Done"
