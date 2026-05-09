# shellcheck shell=bash
# Sourced by install and update. Expects SCRIPT_DIR to be set.

INSTALLED_NODE_TOOLS=(
    linear-notifications
)

NOT_INSTALLED_NODE_TOOLS=(
    comparison-typescript-cli-arguments
)

NODE_TOOLS=(
    "${INSTALLED_NODE_TOOLS[@]}"
    "${NOT_INSTALLED_NODE_TOOLS[@]}"
)

INSTALLED_RUST_TOOLS=(
    assistant
    cloudwatch-insights
    gh-pr
    git-branch-assistant
    git-dirty-checker
    intellij-patch
    log-jsonify
    log-viewer
    package-json-merge
    simons-misc-helpers
    sync-brewfile
    tracker
    x-java-home
)

NOT_INSTALLED_RUST_TOOLS=(
    protobuf-text-to-json
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
        # If a Rust crate ships an embedded TS sub-package (currently only
        # log-viewer/browser), type-check / lint it too. The crate itself
        # is checked without the `web` feature so contributors don't need
        # GTK/webkit2gtk dev libs to run ./check.
        if [[ -f browser/package.json5 ]]; then
            (cd browser && pnpm install && pnpm run check)
        fi
        cargo clippy --all-targets -- -D warnings
        cargo test
    )
}

check-rust-workspace() {
    echo "==> Checking Rust workspace"
    (
        cd "$SCRIPT_DIR"
        # Crates with embedded TS sub-packages (currently log-viewer/browser)
        # need their own type-check / lint pass. The Rust workspace itself
        # is checked without the `web` feature so contributors don't need
        # GTK/webkit2gtk dev libs to run ./check.
        for tool in "${RUST_TOOLS[@]}"; do
            if [[ -f "$tool/browser/package.json5" ]]; then
                (cd "$tool/browser" && pnpm install && pnpm run check)
            fi
        done
        cargo clippy --workspace --all-targets -- -D warnings
        cargo test --workspace
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
    if [[ "$dir" == "log-viewer" ]]; then
        # log-viewer's --web mode embeds the React app from browser/web/dist
        # into the Rust binary via include_dir!. The crate's build.rs runs
        # `pnpm install && pnpm run build:web` in browser/ automatically when
        # the `web` feature is on, so all this special case has to do is
        # turn the feature on.
        (cd "$SCRIPT_DIR/$dir" && cargo install --path . --bin "$dir" --features web)
        return
    fi
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

update-rust-workspace() {
    echo "==> Updating Rust workspace"
    (cd "$SCRIPT_DIR" && cargo update)
}

update-maven() {
    local dir="$1"
    echo "==> Updating $dir"
    (cd "$SCRIPT_DIR/$dir" && mvn versions:use-latest-releases)
}

