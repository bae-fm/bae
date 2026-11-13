#!/bin/bash
# Clippy over the workspace crates named as `-p <crate>` arguments: every
# target, warnings as errors, with the test features each crate declares
# (`test-utils`, `test-instrumentation`) and every feature bae-bridge declares
# (the set the macOS Debug app builds it with, bae-bridge/build-macos.sh), so
# the tests and the desktop code are compiled. Runs as the pre-commit hook's
# compile check. `--workspace` checks every workspace crate in one run, with the
# features of all of them (one feature resolution), as CI does.
#
# Only the named crates are linted. The workspace crates they depend on are
# compiled by scripts/clippy-rustc-wrapper.sh through sccache: under
# clippy-driver sccache caches nothing, so each new set of crates (a new
# feature resolution) would recheck every workspace crate below them. They were
# linted when a commit changed them.
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
# A crate scripts/clippy-rustc-wrapper.sh compiled without lints is fresh to
# cargo, which would then never lint it. Its records name the unit's folder in
# .fingerprint; dating that folder's files to 1970 makes cargo see every input
# of the unit as newer than its last build, so it rebuilds, and lints, each unit
# of a crate named here.
for crate in "${crates[@]+"${crates[@]}"}"; do
    for record in target/*/clippy-unlinted/"$crate"__*; do
        [ -f "$record" ] || continue
        fingerprint=$(cat "$record")
        if [ -d "$fingerprint" ]; then
            find "$fingerprint" -type f -exec touch -t 197001020000 {} +
        fi
        rm -f "$record"
    done
done
RUSTC_WRAPPER="$PWD/scripts/clippy-rustc-wrapper.sh" exec scripts/cargo.sh "${args[@]}" -- -D warnings
