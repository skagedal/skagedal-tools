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
    disky
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

# Swift packages. macOS-only — appicon-generator draws through AppKit and Core
# Text — so ./check skips them anywhere else, and CI runs them on a separate
# macOS job.
INSTALLED_SWIFT_TOOLS=(
    appicon-generator
)

SWIFT_TOOLS=(
    "${INSTALLED_SWIFT_TOOLS[@]}"
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
        cargo fmt --check
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
        cargo fmt --all --check
        cargo clippy --workspace --all-targets -- -D warnings
        cargo test --workspace
    )
}

# Where Swift binaries get installed. Rust tools go to ~/.cargo/bin by way of
# cargo install and Node tools are pnpm-linked; SwiftPM has no equivalent, so
# the release binary is copied to ~/.local/bin, which is already on PATH.
SWIFT_BIN_DIR="$HOME/.local/bin"

swift-available() {
    if ! [[ "$(uname -s)" == "Darwin" ]]; then
        return 1
    fi
    command -v swift >/dev/null 2>&1
}

check-swift() {
    local dir="$1"
    if ! swift-available; then
        echo "==> Skipping $dir (needs a Swift toolchain on macOS)"
        return 0
    fi
    echo "==> Checking $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        # --strict fails on lint findings rather than just printing them, which
        # is what makes this a check rather than a report.
        swift format lint --strict --recursive --parallel Package.swift Sources Tests
        swift build
        swift test
    )
}

install-swift() {
    local dir="$1"
    echo "==> Installing $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        swift build --configuration release
        mkdir -p "$SWIFT_BIN_DIR"
        install -m 0755 "$(swift build --configuration release --show-bin-path)/$dir" \
            "$SWIFT_BIN_DIR/$dir"
        echo "    installed to $SWIFT_BIN_DIR/$dir"
    )
}

update-swift() {
    local dir="$1"
    echo "==> Updating $dir"
    (cd "$SCRIPT_DIR/$dir" && swift package update)
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
    # Reuse the workspace target/ across installs so common deps (clap,
    # serde, tokio, …) are compiled once instead of from scratch in a
    # fresh tmpdir for every tool. Works for both bulk install and
    # `./install <single-tool>` — subsequent single installs are also
    # faster because dep artifacts persist in target/release/.
    export CARGO_TARGET_DIR="$SCRIPT_DIR/target"
    if [[ "$dir" == "log-viewer" ]]; then
        # log-viewer's --web mode embeds the React app from browser/web/dist
        # into the Rust binary via include_dir!. The crate's build.rs runs
        # `pnpm install && pnpm run build:web` in browser/ automatically when
        # the `web` feature is on, so all this special case has to do is
        # turn the feature on.
        (cd "$SCRIPT_DIR" && cargo install --path "$dir" --bin "$dir" --features web)
        return
    fi
    (cd "$SCRIPT_DIR" && cargo install --path "$dir" --bin "$dir")
}

update-node() {
    local dir="$1"
    echo "==> Updating $dir"
    (cd "$SCRIPT_DIR/$dir" && pnpm update --no-save)
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

