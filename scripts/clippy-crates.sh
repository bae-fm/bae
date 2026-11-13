#!/bin/bash
# Clippy over the workspace crates named as `-p <crate>` arguments: every
# target, warnings as errors, with the test features each crate declares
# (`test-utils`, `test-instrumentation`) and every feature bae-bridge declares
# (the set the macOS Debug app builds it with, bae-bridge/build-macos.sh), so
# the tests and the desktop code are compiled. Runs as the pre-commit hook's
# compile check. `--workspace` checks every workspace crate in one run, with the
# features of all of them (one feature resolution), as CI does.
set -euo pipefail
cd "$(dirname "$0")/.."

crates=()
workspace=0
if [ "$#" -eq 1 ] && [ "$1" = "--workspace" ]; then
    workspace=1
else
    while [ "$#" -gt 0 ]; do
        [ "$1" = "-p" ] && [ "$#" -ge 2 ] || { echo "usage: $0 [--workspace | -p <crate>...]" >&2; exit 2; }
        crates+=("$2")
        shift 2
    done
    [ "${#crates[@]}" -gt 0 ] || exit 0
fi

features=$(
    cargo metadata --format-version 1 --offline --no-deps | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
wanted = set(sys.argv[1:])
for package in metadata["packages"]:
    name = package["name"]
    if wanted and name not in wanted:
        continue
    for feature in sorted(package["features"]):
        if feature in ("test-utils", "test-instrumentation") or (
            name == "bae-bridge" and feature != "default"
        ):
            print(f"{name}/{feature}")
' "${crates[@]+"${crates[@]}"}"
)

args=(clippy --all-targets)
if [ "$workspace" -eq 1 ]; then
    args+=(--workspace)
fi
for crate in "${crates[@]+"${crates[@]}"}"; do
    args+=(-p "$crate")
done
while IFS= read -r feature; do
    [ -n "$feature" ] && args+=(--features "$feature")
done <<< "$features"
exec scripts/cargo.sh "${args[@]}" -- -D warnings
