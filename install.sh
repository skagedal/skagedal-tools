#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CHECK=0
REQUESTED_TOOLS=()
for arg in "$@"; do
    case "$arg" in
        --check)
            CHECK=1
            ;;
        -*)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--check] [tool ...]" >&2
            exit 1
            ;;
        *)
            REQUESTED_TOOLS+=("$arg")
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
    (cd "$SCRIPT_DIR/$dir" && cargo install --path .)
}

NODE_TOOLS=(linear-notifications cloudwatch-insights)
RUST_TOOLS=(git-dirty-checker log-jsonify x-java-home)

tool-kind() {
    local tool="$1"
    for t in "${NODE_TOOLS[@]}"; do
        if [[ "$t" == "$tool" ]]; then
            echo "node"
            return 0
        fi
    done
    for t in "${RUST_TOOLS[@]}"; do
        if [[ "$t" == "$tool" ]]; then
            echo "rust"
            return 0
        fi
    done
    return 1
}

if [[ ${#REQUESTED_TOOLS[@]} -gt 0 ]]; then
    SELECTED_NODE_TOOLS=()
    SELECTED_RUST_TOOLS=()
    for tool in "${REQUESTED_TOOLS[@]}"; do
        kind="$(tool-kind "$tool")" || {
            echo "Unknown tool: $tool" >&2
            echo "Available tools: ${NODE_TOOLS[*]} ${RUST_TOOLS[*]}" >&2
            exit 1
        }
        case "$kind" in
            node) SELECTED_NODE_TOOLS+=("$tool") ;;
            rust) SELECTED_RUST_TOOLS+=("$tool") ;;
        esac
    done
else
    SELECTED_NODE_TOOLS=("${NODE_TOOLS[@]}")
    SELECTED_RUST_TOOLS=("${RUST_TOOLS[@]}")
fi

if [[ $CHECK -eq 1 ]]; then
    for tool in "${SELECTED_NODE_TOOLS[@]}"; do
        check-node "$tool"
    done
    for tool in "${SELECTED_RUST_TOOLS[@]}"; do
        check-rust "$tool"
    done
fi

for tool in "${SELECTED_NODE_TOOLS[@]}"; do
    install-node "$tool"
done
for tool in "${SELECTED_RUST_TOOLS[@]}"; do
    install-rust "$tool"
done

echo "==> Done"
