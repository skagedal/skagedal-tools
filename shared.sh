# shellcheck shell=bash
# Sourced by check, install and ci/update-dependencies. Expects SCRIPT_DIR to be set.

# These scripts each do one full pass over the workspace, so incremental
# compilation has nothing to be incremental against — it just leaves another
# few hundred MB in target/debug. Only set here, so an interactive
# `cargo build` in a crate directory keeps its incremental cache.
export CARGO_INCREMENTAL=0

# Ceiling for the shared target/ directory, in MB. Everything in there is
# cache — the binaries that matter are copied out to ~/.cargo/bin and
# ~/.local/bin — so the only cost of dropping an artifact is rebuild time.
TARGET_MAXSIZE_MB="${SKAGEDAL_TOOLS_TARGET_MAXSIZE_MB:-1500}"

# pnpm 12 looks for package.json and package.yaml only, so it cannot see the
# package.json5 manifests the Node tools use — an install there is a silent
# no-op and every `pnpm run` reports a missing script. When the pnpm on PATH
# is one of those, put a shim for the pinned version in front of it. The call
# sites below keep saying `pnpm`, and so does log-viewer's build.rs, which
# spawns it as a subprocess of cargo and so cannot see a shell function.
PNPM_VERSION="10.33.0"

pnpm-reads-package-json5() {
    local version
    version="$(pnpm --version 2>/dev/null)" || return 1
    [[ "$version" =~ ^([0-9]+)\. ]] || return 1
    (( BASH_REMATCH[1] < 12 ))
}

shim-pnpm() {
    PNPM_SHIM_DIR="$(mktemp -d)"
    trap 'rm -rf "$PNPM_SHIM_DIR"' EXIT
    cat >"$PNPM_SHIM_DIR/pnpm" <<EOF
#!/usr/bin/env bash
exec npx --yes "pnpm@$PNPM_VERSION" "\$@"
EOF
    chmod +x "$PNPM_SHIM_DIR/pnpm"
    export PATH="$PNPM_SHIM_DIR:$PATH"
    # pnpm 12 keeps its global shims in $PNPM_HOME/bin and puts that on PATH,
    # where pnpm 10 links into $PNPM_HOME itself and then refuses to link at
    # all because that directory is not on PATH. Aim the older one at the
    # directory that is. Only `pnpm link --global` needs it, and npx warns
    # about the unknown npm config, so it is set on that call alone.
    if [[ -n "${PNPM_HOME:-}" ]]; then
        PNPM_GLOBAL_BIN_DIR="$PNPM_HOME/bin"
    fi
}

pnpm-reads-package-json5 || shim-pnpm

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
    chrome-page-notes
    cloudwatch-insights
    disky
    gh-pr
    git-branch-assistant
    git-dirty-checker
    intellij-patch
    kontoutdrag
    log-jsonify
    log-viewer
    package-json-merge
    simons-misc-helpers
    sync-brewfile
    tracker
    trafikverket
    woke
    x-java-home
)

NOT_INSTALLED_RUST_TOOLS=(
    protobuf-text-to-json
)

RUST_TOOLS=(
    "${INSTALLED_RUST_TOOLS[@]}"
    "${NOT_INSTALLED_RUST_TOOLS[@]}"
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
        if [[ -f browser/package.json ]]; then
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
            if [[ -f "$tool/browser/package.json" ]]; then
                (cd "$tool/browser" && pnpm install && pnpm run check)
            fi
        done
        cargo fmt --all --check
        cargo clippy --workspace --all-targets -- -D warnings
        cargo test --workspace
    )
}

# Tools that ship a `completions` subcommand. Kept as a list rather than
# sniffed out of --help, which works right up until someone rewords a line.
COMPLETION_TOOLS=(
    assistant
    tracker
)

# Where zsh completion functions go. This one is on the fpath in my dotfiles
# (shell/zshrc.sh); override it for another layout.
ZSH_COMPLETIONS_DIR="${ZSH_COMPLETIONS_DIR:-$HOME/local/zsh-functions}"

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

install-node() {
    local dir="$1"
    echo "==> Installing $dir"
    (
        cd "$SCRIPT_DIR/$dir"
        pnpm install
        pnpm run build
        if [[ -n "${PNPM_GLOBAL_BIN_DIR:-}" ]]; then
            npm_config_global_bin_dir="$PNPM_GLOBAL_BIN_DIR" pnpm link --global
        else
            pnpm link --global
        fi
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
    if [[ "$dir" == "log-viewer" || "$dir" == "kontoutdrag" ]]; then
        # Their webviews embed a React app from browser/ into the binary via
        # include_dir!. Each crate's build.rs runs pnpm and Vite in browser/
        # automatically when the `web` feature is on, so all this special
        # case has to do is turn the feature on.
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

# Drop the oldest build artifacts until target/ is back under the ceiling.
# Oldest-first is what makes this cheap: the artifacts a subsequent build
# wants are the ones it keeps.
# is-selected NAME — whether NAME is part of this run, so that
# `./install tracker` regenerates only tracker's completions.
is-selected() {
    local name="$1" t
    for t in ${SELECTED_NODE_TOOLS[@]+"${SELECTED_NODE_TOOLS[@]}"} \
             ${SELECTED_RUST_TOOLS[@]+"${SELECTED_RUST_TOOLS[@]}"} \
             ${SELECTED_SWIFT_TOOLS[@]+"${SELECTED_SWIFT_TOOLS[@]}"}; do
        [[ "$t" == "$name" ]] && return 0
    done
    return 1
}

# install-completions — regenerate zsh completions for the selected tools that
# offer them. cargo install has no post-install hook (build.rs runs at build
# time and cannot know where the binary lands), so it belongs here, in the
# script that is actually run after changing a tool.
install-completions() {
    local tool target tmp generated=0

    for tool in "${COMPLETION_TOOLS[@]}"; do
        is-selected "$tool" || continue
        command -v "$tool" >/dev/null 2>&1 || continue

        mkdir -p "$ZSH_COMPLETIONS_DIR"
        target="${ZSH_COMPLETIONS_DIR}/_${tool}"
        tmp="$(mktemp)"
        if "$tool" completions zsh > "$tmp" 2>/dev/null && [[ -s "$tmp" ]]; then
            mv "$tmp" "$target"
            chmod 644 "$target"
            echo "    ${target}"
            generated=$((generated + 1))
        else
            rm -f "$tmp"
            echo "    ${tool}: completions zsh produced nothing" >&2
        fi
    done

    [[ "$generated" -gt 0 ]] || return 0

    echo "==> Wrote ${generated} zsh completion file(s)"

    # An interactive shell that runs `compinit -C` trusts its cached dump and
    # never notices a new file, so rebuild it rather than leave a completion
    # that only starts working tomorrow.
    #
    # `zsh -i`, not plain `zsh -c`: .zshrc is only sourced for interactive
    # shells, so a non-interactive one does not have the completions directory
    # on its fpath and would cheerfully write a dump with these files missing —
    # which is worse than not rebuilding at all, since the stale dump then
    # looks fresh. The directory is also added explicitly, in case a layout
    # keeps it somewhere .zshrc does not put on the fpath.
    if command -v zsh >/dev/null 2>&1; then
        zsh -i -c "fpath=(${ZSH_COMPLETIONS_DIR} \$fpath); autoload -Uz compinit && compinit" \
            >/dev/null 2>&1 \
            || echo "    (could not rebuild the completion dump)" >&2
    fi
}

sweep-target() {
    if ! command -v cargo-sweep >/dev/null 2>&1; then
        echo "==> Skipping target/ sweep (cargo-sweep not installed)"
        return 0
    fi
    echo "==> Sweeping target/ down to ${TARGET_MAXSIZE_MB} MB"
    cargo sweep --maxsize "$TARGET_MAXSIZE_MB" "$SCRIPT_DIR"
}
