#!/usr/bin/env bash
# Register the repo's git merge drivers in the shared git config (every
# worktree reads it). Run once in an existing checkout; post-checkout runs it
# for a new worktree. Merges, rebases and cherry-picks all use the driver, and
# run from the worktree's top level, so the path is relative.
set -euo pipefail
git config merge.xcstrings.name "Xcode string catalog, merged by key"
git config merge.xcstrings.driver "python3 scripts/merge-xcstrings.py %O %A %B"
