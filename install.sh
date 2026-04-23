#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

install-rust() {
    local dir="$1"
    echo "==> Installing $dir"
    (cd "$SCRIPT_DIR/$dir" && cargo install --path .)
}

echo "==> Installing linear-notifications"
(cd "$SCRIPT_DIR/linear-notifications" && pnpm link --global)

install-rust git-dirty-checker
install-rust log-jsonify
install-rust x-java-home

echo "==> Done"
