#!/usr/bin/env bash
set -euo pipefail

create-temp-dir() {
    local dir
    dir=$(mktemp -d)
    echo "$dir"
}

create-no-upstream() {
    local upstreams
    local repos

    upstreams="$1"
    repos="$2"

    testcase="no-upstream"

    git init -q "${upstreams}/${testcase}"

    cd "${upstreams}/${testcase}"
    git config commit.gpgsign false
    echo "Initial commit" > file.txt
    git add file.txt
    git commit -q -m "Initial commit"
    git checkout --detach

    cd "${repos}"
    git clone "${upstreams}/${testcase}" "$testcase"

    cd "${repos}/${testcase}"
    git config commit.gpgsign false
    git checkout -b new-branch
    echo "New branch commit" >> file.txt
    git add file.txt
    git commit -q -m "New branch commit"
}

create-branches-identical() {
    local upstreams
    local repos

    upstreams="$1"
    repos="$2"

    testcase="branches-identical"

    git init -q "${upstreams}/${testcase}"

    cd "${upstreams}/${testcase}"
    git config commit.gpgsign false
    echo "Initial commit" > file.txt
    git add file.txt
    git commit -q -m "Initial commit"
    git checkout --detach

    cd "${repos}"
    git clone "${upstreams}/${testcase}" "$testcase"

    cd "${repos}/${testcase}"
    git fetch origin
}

create-upstream-ahead() {
    local upstreams
    local repos

    upstreams="$1"
    repos="$2"

    testcase="upstream-ahead"

    git init -q "${upstreams}/${testcase}"

    cd "${upstreams}/${testcase}"
    git config commit.gpgsign false
    echo "Initial commit" > file.txt
    git add file.txt
    git commit -q -m "Initial commit"

    cd "${repos}"
    git clone "${upstreams}/${testcase}" "$testcase"

    cd "${upstreams}/${testcase}"
    echo "Upstream commit 1" >> file.txt
    git add file.txt
    git commit -q -m "Upstream commit 1"
    git checkout --detach

    cd "${repos}/${testcase}"
    git fetch origin
}

create-local-ahead() {
    local upstreams
    local repos

    upstreams="$1"
    repos="$2"

    testcase="local-ahead"

    git init -q "${upstreams}/${testcase}"

    cd "${upstreams}/${testcase}"
    git config commit.gpgsign false
    echo "Initial commit" > file.txt
    git add file.txt
    git commit -q -m "Initial commit"
    git checkout --detach

    cd "${repos}"
    git clone "${upstreams}/${testcase}" "$testcase"

    cd "${repos}/${testcase}"
    git config commit.gpgsign false
    echo "Local commit 1" >> file.txt
    git add file.txt
    git commit -q -m "Local commit 1"
}

create-diverged-branches() {
    local upstreams
    local repos

    upstreams="$1"
    repos="$2"

    testcase="diverged-branches"

    git init -q "${upstreams}/${testcase}"

    cd "${upstreams}/${testcase}"
    git config commit.gpgsign false
    echo "Initial commit" > file.txt
    git add file.txt
    git commit -q -m "Initial commit"

    cd "${repos}"
    git clone "${upstreams}/${testcase}" "$testcase"

    cd "${upstreams}/${testcase}"
    echo "Upstream changes" > upstream-file.txt
    git add upstream-file.txt
    git commit -q -m "Upstream commit 1"
    git checkout --detach

    cd "${repos}/${testcase}"
    git config commit.gpgsign false
    echo "Local changes" > local-file.txt
    git add local-file.txt
    git commit -q -m "Local commit 1"
    git fetch origin
}

create-upstream-missing() {
    local upstreams
    local repos

    upstreams="$1"
    repos="$2"

    testcase="upstream-missing"

    git init -q "${upstreams}/${testcase}"

    cd "${upstreams}/${testcase}"
    git config commit.gpgsign false
    echo "Initial commit" > file.txt
    git add file.txt
    git commit -q -m "Initial commit"
    git checkout --detach

    cd "${repos}"
    git clone "${upstreams}/${testcase}" "$testcase"

    cd "${repos}/${testcase}"
    git config commit.gpgsign false
    git checkout -b feature-branch
    echo "Feature commit" >> file.txt
    git add file.txt
    git commit -q -m "Feature commit"

    # Push the branch to create it on the remote, then delete it
    git push -u origin feature-branch
    git push origin --delete feature-branch
}

temp_dir=$(create-temp-dir)
upstreams="${temp_dir}/upstreams"
repos="${temp_dir}/repos"
mkdir -p "$upstreams"
mkdir -p "$repos"

echo "Generating test cases..."
create-no-upstream "$upstreams" "$repos"
echo "  ✓ no-upstream"
create-branches-identical "$upstreams" "$repos"
echo "  ✓ branches-identical"
create-upstream-ahead "$upstreams" "$repos"
echo "  ✓ upstream-ahead"
create-local-ahead "$upstreams" "$repos"
echo "  ✓ local-ahead"
create-diverged-branches "$upstreams" "$repos"
echo "  ✓ diverged-branches"
create-upstream-missing "$upstreams" "$repos"
echo "  ✓ upstream-missing"

echo ""
echo "Test cases created in: $repos"
echo ""
echo "Available test cases:"
echo "  - no-upstream: Branch without an upstream"
echo "  - branches-identical: Local and upstream point to same commit"
echo "  - upstream-ahead: Upstream is ahead of local branch"
echo "  - local-ahead: Local branch is ahead of upstream"
echo "  - diverged-branches: Branches have diverged"
echo "  - upstream-missing: Upstream is set but doesn't exist"
