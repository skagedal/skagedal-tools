#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CHECK=0
for arg in "$@"; do
    case "$arg" in
        --check)
            CHECK=1
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--check]" >&2
            exit 1
            ;;
    esac
done

check-node() {
    local dir="$1"
    echo "==> Testing $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        pnpm install
        if node -e "const p=require('./package.json'); process.exit(p.scripts && p.scripts.test ? 0 : 1)"; then
            pnpm test
        else
            echo "    (no test script defined, skipping)"
        fi
    )
}

check-rust() {
    local dir="$1"
    echo "==> Testing $dir"
    (cd "$SCRIPT_DIR/$dir" && cargo test)
}

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

NODE_TOOLS=(linear-notifications cloudwatch-insights)
RUST_TOOLS=(git-dirty-checker log-jsonify gh-pr x-java-home)

if [[ $CHECK -eq 1 ]]; then
    for tool in "${NODE_TOOLS[@]}"; do
        check-node "$tool"
    done
    for tool in "${RUST_TOOLS[@]}"; do
        check-rust "$tool"
    done
fi

for tool in "${NODE_TOOLS[@]}"; do
    install-node "$tool"
done
for tool in "${RUST_TOOLS[@]}"; do
    install-rust "$tool"
done

echo "==> Done"
