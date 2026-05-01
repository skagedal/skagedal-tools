# shellcheck shell=bash
# Sourced by install and update. Expects SCRIPT_DIR to be set.

INSTALLED_NODE_TOOLS=(
    linear-notifications
    log-viewer
)

NOT_INSTALLED_NODE_TOOLS=(
    comparison-typescript-cli-arguments
)

NODE_TOOLS=(
    "${INSTALLED_NODE_TOOLS[@]}"
    "${NOT_INSTALLED_NODE_TOOLS[@]}"
)

INSTALLED_RUST_TOOLS=(
    cloudwatch-insights
    gh-pr
    git-dirty-checker
    intellij-patch
    log-jsonify
    package-json-merge
    sync-brewfile
    x-java-home
)

NOT_INSTALLED_RUST_TOOLS=(
    protobuf-text-to-json
    rust-log-viewer
)

RUST_TOOLS=(
    "${INSTALLED_RUST_TOOLS[@]}"
    "${NOT_INSTALLED_RUST_TOOLS[@]}"
)

MAVEN_TOOLS=(
    git-repos-latest-activity
)

check-node() {
    local dir="$1"
    echo "==> Checking $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        pnpm install
        pnpm run check
    )
}

check-rust() {
    local dir="$1"
    echo "==> Checking $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        cargo clippy --all-targets -- -D warnings
        cargo test
    )
}

check-maven() {
    local dir="$1"
    echo "==> Checking $dir"
    (cd "$SCRIPT_DIR/$dir" && mvn --batch-mode test)
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

update-node() {
    local dir="$1"
    echo "==> Updating $dir"
    (cd "$SCRIPT_DIR/$dir" && pnpm update)
}

update-rust() {
    local dir="$1"
    echo "==> Updating $dir"
    (cd "$SCRIPT_DIR/$dir" && cargo update)
}

update-maven() {
    local dir="$1"
    echo "==> Updating $dir"
    (cd "$SCRIPT_DIR/$dir" && mvn versions:use-latest-releases)
}

