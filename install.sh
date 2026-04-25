#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

install-node() {
    local dir="$1"
    echo "==> Installing $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        pnpm install
        pnpm run build
        pnpm link --global
    )
}

install-rust() {
    local dir="$1"
    echo "==> Installing $dir"
    (cd "$SCRIPT_DIR/$dir" && cargo install --path . --bin "$dir")
}

install-node linear-notifications
install-node cloudwatch-insights

install-rust git-dirty-checker
install-rust log-jsonify
install-rust gh-pr
install-rust x-java-home

echo "==> Done"
