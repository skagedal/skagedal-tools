# shellcheck shell=bash
# Sourced by install and update. Expects SCRIPT_DIR to be set.

NODE_TOOLS=(
    cloudwatch-insights
    comparison-typescript-cli-arguments
    linear-notifications
    log-viewer
)

RUST_TOOLS=(
    gh-pr
    git-dirty-checker
    log-jsonify
    protobuf-text-to-json
    sync-brewfile
    x-java-home
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

