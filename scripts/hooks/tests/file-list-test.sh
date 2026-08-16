#!/bin/bash

set -e

source "$(dirname "$0")/../lib/file-list.sh"

CHANGED_FILES=$'bae-core/Cargo.toml\nbae-core/src/lib.rs'

if ! staged_file_in_list "bae-core/Cargo.toml"; then
    echo "expected the exact staged path to match"
    exit 1
fi

if staged_file_in_list "Cargo.toml"; then
    echo "a path suffix matched a different staged file"
    exit 1
fi

repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
git -C "$repo" init -q
git -C "$repo" config user.email "test@example.invalid"
git -C "$repo" config user.name "Hook Test"
printf 'base\n' > "$repo/partial.txt"
printf 'base\n' > "$repo/clean.txt"
git -C "$repo" add partial.txt clean.txt
git -C "$repo" commit -qm base

run_stage_formatter_changes() {
    local state_file="$1"
    local expected_status="$2"
    set +e
    stage_formatter_changes "$state_file"
    local status=$?
    set -e
    if [ "$status" -ne "$expected_status" ]; then
        echo "formatter staging returned $status, expected $expected_status"
        exit 1
    fi
}

cd "$repo"

# An unstaged hunk that existed before formatting is not formatter output.
printf 'staged\n' > partial.txt
git add partial.txt
printf 'staged\npreexisting\n' > partial.txt
CHANGED_FILES=partial.txt
partial_state=$(mktemp)
record_staged_worktree_state "$partial_state"
partial_index_before=$(git rev-parse :partial.txt)
run_stage_formatter_changes "$partial_state" 0
if [ "$(git rev-parse :partial.txt)" != "$partial_index_before" ]; then
    echo "preexisting unstaged content entered the index"
    exit 1
fi

# Formatter output on an otherwise fully staged file is staged, then reported.
printf 'staged\n' > clean.txt
git add clean.txt
CHANGED_FILES=clean.txt
clean_state=$(mktemp)
record_staged_worktree_state "$clean_state"
printf 'formatted\n' > clean.txt
run_stage_formatter_changes "$clean_state" 1
if ! git diff --quiet -- clean.txt; then
    echo "formatter output on a fully staged file was not staged"
    exit 1
fi

# Formatter output on a partially staged file fails without changing its index.
CHANGED_FILES=partial.txt
record_staged_worktree_state "$partial_state"
partial_index_before=$(git rev-parse :partial.txt)
printf 'formatted\npreexisting\n' > partial.txt
run_stage_formatter_changes "$partial_state" 2
if [ "$(git rev-parse :partial.txt)" != "$partial_index_before" ]; then
    echo "formatter output swept a partial file into the index"
    exit 1
fi
