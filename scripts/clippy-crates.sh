#!/bin/bash
# Clippy over the workspace crates named as `-p <crate>` arguments: every
# target, warnings as errors, with the test features each crate declares
# (`test-utils`, `test-instrumentation`) and every feature bae-bridge declares
# (the set the macOS Debug app builds it with, bae-bridge/build-macos.sh), so
# the tests and the desktop code are compiled. Runs as the pre-commit hook's
# compile check.
set -euo pipefail
cd "$(dirname "$0")/.."

crates=()
while [ "$#" -gt 0 ]; do
    [ "$1" = "-p" ] && [ "$#" -ge 2 ] || { echo "usage: $0 [-p <crate>]..." >&2; exit 2; }
    crates+=("$2")
    shift 2
done
[ "${#crates[@]}" -gt 0 ] || exit 0

features=$(
    cargo metadata --format-version 1 --offline --no-deps | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
wanted = set(sys.argv[1:])
for package in metadata["packages"]:
    name = package["name"]
    if name not in wanted:
        continue
    for feature in sorted(package["features"]):
        if feature in ("test-utils", "test-instrumentation") or (
            name == "bae-bridge" and feature != "default"
        ):
            print(f"{name}/{feature}")
' "${crates[@]}"
)

args=(clippy --all-targets)
for crate in "${crates[@]}"; do
    args+=(-p "$crate")
done
while IFS= read -r feature; do
    [ -n "$feature" ] && args+=(--features "$feature")
done <<< "$features"
exec scripts/cargo.sh "${args[@]}" -- -D warnings
