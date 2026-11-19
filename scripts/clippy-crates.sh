#!/bin/bash
# Clippy, with the typed lints beside clippy's own, over the workspace crates
# named as `-p <crate>` arguments: every target, warnings as errors, with the
# test features each crate declares (`test-utils`, `test-instrumentation`) and
# every feature bae-bridge declares (the set the macOS Debug app builds it
# with, bae-bridge/build-macos.sh), so the tests and the desktop code are
# compiled. Runs as the pre-commit hook's compile check. `--workspace` checks
# every workspace crate in one run, with the features of all of them (one
# feature resolution), as CI does. tools/typed-lints/clippy.sh runs clippy, so
# each crate compiles once for clippy's lints and the typed lints.
#
# Only the named crates are linted. The workspace crates they depend on
# compile through the typed lints' driver too, which writes the record the
# read rule's lints read in the crates after them, and report nothing
# (scripts/clippy-rustc-wrapper.sh). They were linted when a commit changed
# them.
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

# The first line says whether the run lints every workspace crate; each line
# after it names a feature to turn on.
plan=$(
    cargo metadata --format-version 1 --offline --no-deps | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
wanted = set(sys.argv[1:])
names = {package["name"] for package in metadata["packages"]}
print("every" if not wanted or names <= wanted else "some")
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
lints_every_crate=$(sed -n 1p <<< "$plan")
features=$(sed 1d <<< "$plan")

args=(--all-targets)
if [ "$workspace" -eq 1 ]; then
    args+=(--workspace)
fi
for crate in "${crates[@]+"${crates[@]}"}"; do
    args+=(-p "$crate")
done
while IFS= read -r feature; do
    [ -n "$feature" ] && args+=(--features "$feature")
done <<< "$features"
# On whichever machine runs cargo (`builds cargo` sends the check to the host
# with the most free cores), builds runs scripts/clippy-before.sh first,
# through BUILDS_CARGO_BEFORE: it points tools/typed-lints/built at the driver
# there, and makes each crate named here stale, since a crate
# scripts/clippy-rustc-wrapper.sh compiled without lints is fresh to cargo,
# which would then never lint it.
export BUILDS_CARGO_BEFORE="scripts/clippy-before.sh ${crates[*]+"${crates[*]}"}"
# clippy has no lint or flag that makes a clippy.toml entry naming nothing an
# error: it prints "warning: `<path>` does not refer to a reachable function"
# (or type, macro, ...), which -D warnings leaves a warning, so a misspelled
# entry would gate nothing. Fail on that text, naming each entry, when the run
# lints every crate: clippy prints it for each linted crate that loads a
# dependency without the feature that declares the entry's function, such as
# tokio's `Runtime::new` in a crate with tokio's `rt` alone, so only a run
# with every crate's features shows that no crate reaches the entry. Every
# crate is linted when clippy.toml changes ({crates} names every crate then).
log=$(mktemp)
trap 'rm -f "$log"' EXIT
status=0
RUSTC_WRAPPER="$PWD/scripts/clippy-rustc-wrapper.sh" \
    tools/typed-lints/clippy.sh "${args[@]}" -- -D warnings 2> >(tee "$log" >&2) || status=$?
wait
unreachable=$(sed $'s/\\x1b\\[[0-9;]*m//g' "$log" | grep -E 'does not refer to a reachable' | sort -u || true)
if [ "$lints_every_crate" = every ] && [ -n "$unreachable" ]; then
    echo "clippy.toml names paths that do not exist (fix or remove these entries):" >&2
    echo "$unreachable" >&2
    exit 1
fi
exit "$status"
