#!/usr/bin/env bash

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# coven's structural checker, pinned to a coven revision. It reads the rules
# it runs, and what it holds them to, from this workspace's
# owner-construction-check.toml. This tool pin is independent of the runtime
# dependency in Cargo.toml.
checker_revision="9e92d464d2ecb0e27c5921f005b6b2e544b76dd5"
# Every checkout on this machine runs the one checker installed for its
# revision. The first checkout that needs a revision builds the checker in its
# own target folder, installs it into a new folder beside the shared ones, and
# renames the executable into place, so no checkout runs a partly written
# checker.
checker_directory="${XDG_CACHE_HOME:-$HOME/.cache}/owner-construction-check"
checker="$checker_directory/$checker_revision"
if [ ! -x "$checker" ]; then
    target_directory="$(cargo metadata --format-version 1 --locked --no-deps | python3 -c '
import json
import sys

print(json.load(sys.stdin)["target_directory"])
')"
    mkdir -p "$checker_directory"
    install_root="$(mktemp -d "$checker_directory/.install.XXXXXX")"
    trap 'rm -rf "$install_root"' EXIT
    scripts/cargo.sh install --locked --git https://github.com/bae-fm/coven \
        --rev "$checker_revision" --root "$install_root" \
        --target-dir "$target_directory/owner-construction-check" owner-construction-check
    mv -f "$install_root/bin/owner-construction-check" "$checker"
fi

"$checker"
