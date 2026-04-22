#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

install_node() {
    local dir="$1"
    echo "==> Installing $dir"
    (cd "$SCRIPT_DIR/$dir" && pnpm link --global)
}

install_rust() {
    local dir="$1"
    echo "==> Installing $dir"
    (cd "$SCRIPT_DIR/$dir" && cargo install --path .)
}

install_node linear-notifications
install_node cloudwatch-insights

install_rust git-dirty-checker
install_rust log-jsonify
install_rust x-java-home

echo "==> Done"
