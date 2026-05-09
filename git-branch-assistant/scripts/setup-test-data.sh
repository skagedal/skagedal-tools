#!/usr/bin/env bash
#
# Set up a temporary directory of git repositories that exercises every
# branch state git-branch-assistant knows about.
#
#   $TMP/upstream/<name>.git    bare "remote" repos
#   $TMP/local/<name>           clones — point `git-branch-assistant repos` here
#
# After it runs, cd into the printed local directory and try, for example:
#
#   git-branch-assistant repos --bulk
#   git-branch-assistant repos --list
#

set -euo pipefail

TMPDIR=$(mktemp -d -t gba-test-data-XXXXXX)
UPSTREAM="$TMPDIR/upstream"
LOCAL="$TMPDIR/local"
WORK="$TMPDIR/work"
mkdir -p "$UPSTREAM" "$LOCAL" "$WORK"

GIT_AUTHOR_NAME="Upstream Author"
GIT_AUTHOR_EMAIL="author@example.com"
GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME"
GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
export GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL

run_git() { git "$@" >/dev/null 2>&1; }

commit_file() {
    local file=$1 content=$2 msg=$3
    printf '%s\n' "$content" > "$file"
    run_git add "$file"
    run_git -c commit.gpgsign=false commit -m "$msg"
}

# init_upstream NAME [BRANCH:FILE:CONTENT ...]
# Creates a bare upstream and seeds it with main + the given feature branches.
init_upstream() {
    local name=$1; shift
    local bare="$UPSTREAM/$name.git"
    run_git init --bare -b main "$bare"

    local seed="$WORK/$name-seed"
    run_git clone "$bare" "$seed"
    pushd "$seed" >/dev/null

    commit_file README.md "# $name" "Initial commit on main"
    commit_file CHANGELOG.md "- bootstrap" "Add changelog"
    run_git push -u origin main

    while [ $# -gt 0 ]; do
        IFS=: read -r branch file content <<<"$1"; shift
        run_git checkout -b "$branch" main
        commit_file "$file" "$content" "Start work on $branch"
        run_git push -u origin "$branch"
        run_git checkout main
    done

    popd >/dev/null
}

clone_local() {
    local name=$1
    run_git clone "$UPSTREAM/$name.git" "$LOCAL/$name"
}

# Run a git command in the upstream's seed work tree, pushing any commits.
in_upstream_seed() {
    local name=$1; shift
    pushd "$WORK/$name-seed" >/dev/null
    run_git fetch origin
    "$@"
    popd >/dev/null
}

# Track an existing remote branch as a local branch so it has an upstream.
track_branch() {
    local name=$1 branch=$2
    pushd "$LOCAL/$name" >/dev/null
    if run_git show-ref --verify "refs/heads/$branch"; then
        run_git checkout "$branch"
    else
        run_git checkout -b "$branch" "origin/$branch"
    fi
    popd >/dev/null
}

# --- Scenario builders ---------------------------------------------------

# A new local branch that was never pushed.
scenario_no_upstream() {
    local name=$1 branch=$2 file=$3 content=$4
    pushd "$LOCAL/$name" >/dev/null
    run_git checkout -b "$branch" main
    commit_file "$file" "$content" "WIP $branch"
    popd >/dev/null
}

# Local has commits the upstream doesn't.
scenario_local_ahead() {
    local name=$1 branch=$2 file=$3 content=$4
    track_branch "$name" "$branch"
    pushd "$LOCAL/$name" >/dev/null
    commit_file "$file" "$content" "Local-only tweak on $branch"
    popd >/dev/null
}

# Upstream has commits local doesn't (local is a fast-forward away).
scenario_upstream_ahead() {
    local name=$1 branch=$2 file=$3 content=$4
    track_branch "$name" "$branch"

    in_upstream_seed "$name" bash -c "
        set -e
        git checkout '$branch' >/dev/null 2>&1 || git checkout -b '$branch' 'origin/$branch' >/dev/null
        printf '%s\n' '$content' > '$file'
        git add '$file' >/dev/null
        git -c commit.gpgsign=false commit -m 'Upstream review notes for $branch' >/dev/null
        git push origin '$branch' >/dev/null 2>&1
    "

    pushd "$LOCAL/$name" >/dev/null
    run_git fetch origin
    popd >/dev/null
}

# Local and upstream both moved on different commits.
scenario_diverged() {
    local name=$1 branch=$2
    track_branch "$name" "$branch"
    pushd "$LOCAL/$name" >/dev/null
    commit_file "${branch}-local.md" "local diverged note" "Diverge locally on $branch"
    popd >/dev/null

    in_upstream_seed "$name" bash -c "
        set -e
        git checkout '$branch' >/dev/null 2>&1 || git checkout -b '$branch' 'origin/$branch' >/dev/null
        printf '%s\n' 'upstream diverged note' > '${branch}-upstream.md'
        git add '${branch}-upstream.md' >/dev/null
        git -c commit.gpgsign=false commit -m 'Diverge upstream on $branch' >/dev/null
        git push origin '$branch' >/dev/null 2>&1
    "

    pushd "$LOCAL/$name" >/dev/null
    run_git fetch origin
    popd >/dev/null
}

# Branch had an upstream, but it was deleted (e.g. PR merged & branch removed).
scenario_upstream_gone() {
    local name=$1 branch=$2
    track_branch "$name" "$branch"
    run_git -C "$UPSTREAM/$name.git" update-ref -d "refs/heads/$branch"
    pushd "$LOCAL/$name" >/dev/null
    run_git fetch --prune origin
    popd >/dev/null
}

# --- Build a bunch of repos ----------------------------------------------

REPO_NAMES=(
    stellar-cartographer
    espresso-firmware
    moss-codex
    abacus-deluxe
    octopus-orchestra
    pinecone-courier
    quasar-archivist
    ravioli-renderer
    saffron-sampler
    tundra-telegraph
)

# Each upstream gets a couple of feature branches with tiny but distinct content.
for name in "${REPO_NAMES[@]}"; do
    init_upstream "$name" \
        "review-comments:notes.md:Initial review notes for $name" \
        "experimental-rewrite:experiment.md:Sketch of $name rewrite" \
        "merged-pr:done.md:Implementation merged in PR for $name"
    clone_local "$name"
done

# Sprinkle scenarios across the repos. Pick a mix so several states show up
# in the same bulk run, but no single repo is overwhelmed.
scenario_no_upstream     stellar-cartographer  wip-nebula        nebula.md     "Sketching out nebula support"
scenario_no_upstream     pinecone-courier      draft-route       route.md      "Routing draft, still rough"
scenario_no_upstream     ravioli-renderer      try-shading       shading.md    "Toy shader experiment"

scenario_local_ahead     stellar-cartographer  review-comments   followup.md   "Address last round of feedback"
scenario_local_ahead     octopus-orchestra     review-comments   addendum.md   "Tighten coda"
scenario_local_ahead     tundra-telegraph      experimental-rewrite extra.md   "Squeeze in one more refactor"

scenario_upstream_ahead  espresso-firmware     review-comments   queue.md      "Add brew queue note"
scenario_upstream_ahead  moss-codex            experimental-rewrite outline.md "Restructure outline"
scenario_upstream_ahead  saffron-sampler       review-comments   spice.md      "Suggest more saffron"

scenario_diverged        abacus-deluxe         review-comments
scenario_diverged        quasar-archivist      experimental-rewrite

scenario_upstream_gone   moss-codex            merged-pr
scenario_upstream_gone   abacus-deluxe         merged-pr
scenario_upstream_gone   octopus-orchestra     merged-pr
scenario_upstream_gone   pinecone-courier      merged-pr
scenario_upstream_gone   tundra-telegraph      merged-pr

cat <<EOF

Test data ready.

  Upstream remotes: $UPSTREAM
  Local clones:     $LOCAL

Try it out:

  cd "$LOCAL"
  git-branch-assistant repos --list
  git-branch-assistant repos --bulk

Cleanup when done:

  rm -rf "$TMPDIR"
EOF
