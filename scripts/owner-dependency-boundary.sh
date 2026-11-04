#!/usr/bin/env bash

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# coven's structural checker, pinned to a coven revision. It reads the rules
# it runs, and what it holds them to, from this workspace's
# owner-construction-check.toml. This tool pin is independent of the runtime
# dependency in Cargo.toml.
checker_revision="04d5224d09082b4d4d21098930fc408e8d7fd863"
target_directory="$(cargo metadata --format-version 1 --locked --no-deps | python3 -c '
import json
import sys

print(json.load(sys.stdin)["target_directory"])
')"
# Isolate the executable and Cargo artifacts by the exact checker revision.
checker_target="$target_directory/owner-construction-check/$checker_revision"
checker_install="$checker_target/install"
if [ ! -x "$checker_install/bin/owner-construction-check" ]; then
    scripts/cargo.sh install --locked --git https://github.com/bae-fm/coven \
        --rev "$checker_revision" --root "$checker_install" \
        --target-dir "$checker_target" owner-construction-check
fi

"$checker_install/bin/owner-construction-check"

python3 scripts/swift-owner-dependency-boundary.py
python3 scripts/ui-projection-boundary.py
python3 scripts/tests/ui-projection-boundary-test.py
python3 scripts/keyed-value-stream-boundary.py
python3 scripts/tests/keyed-value-stream-boundary-test.py
