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
# A second run then lints the same crates' libraries and binaries without the
# test features, which is how the bridge and the apps build them: a function
# that only test code calls has a caller in the first run and is dead code in
# theirs. The second run turns on each named crate's features that a release
# build of the apps turns on: every bae-bridge feature but the two that only
# Debug builds add (`test-keyring` and `dev-reset`), and the workspace features
# those turn on. cargo turns on what a feature implies in the crates below.
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
# after it names a feature to turn on, `test <feature>` for the run over every
# target and `app <feature>` for the run without test features.
plan=$(
    cargo metadata --format-version 1 --offline --no-deps | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
packages = {package["name"]: package for package in metadata["packages"]}
wanted = set(sys.argv[1:]) or set(packages)
print("every" if packages.keys() <= wanted else "some")
for name in sorted(wanted):
    for feature in sorted(packages[name]["features"]):
        if feature in ("test-utils", "test-instrumentation") or (
            name == "bae-bridge" and feature != "default"
        ):
            print(f"test {name}/{feature}")

released, pending = set(), [
    ("bae-bridge", feature) for feature in packages["bae-bridge"]["features"]
    if feature not in ("default", "test-keyring", "dev-reset")]
while pending:
    name, feature = pending.pop()
    if (name, feature) in released:
        continue
    released.add((name, feature))
    for entry in packages[name]["features"][feature]:
        owner, slash, inner = entry.partition("/")
        if slash and owner in packages:
            pending.append((owner, inner))
        elif entry in packages[name]["features"]:
            pending.append((name, entry))
for name, feature in sorted(released):
    if name in wanted:
        print(f"app {name}/{feature}")
' "${crates[@]+"${crates[@]}"}"
)
lints_every_crate=$(sed -n 1p <<< "$plan")

selection=()
if [ "$workspace" -eq 1 ]; then
    selection+=(--workspace)
fi
for crate in "${crates[@]+"${crates[@]}"}"; do
    selection+=(-p "$crate")
done
test_args=(--all-targets "${selection[@]}")
# With no target named, cargo builds each crate's library and binaries.
app_args=("${selection[@]}")
while read -r kind feature; do
    case "$kind" in
        test) test_args+=(--features "$feature") ;;
        app) app_args+=(--features "$feature") ;;
    esac
done <<< "$(sed 1d <<< "$plan")"
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
    tools/typed-lints/clippy.sh "${test_args[@]}" -- -D warnings 2> >(tee "$log" >&2) || status=$?
wait
unreachable=$(sed $'s/\\x1b\\[[0-9;]*m//g' "$log" | grep -E 'does not refer to a reachable' | sort -u || true)
if [ "$lints_every_crate" = every ] && [ -n "$unreachable" ]; then
    echo "clippy.toml names paths that do not exist (fix or remove these entries):" >&2
    echo "$unreachable" >&2
    exit 1
fi
[ "$status" -eq 0 ] || exit "$status"
RUSTC_WRAPPER="$PWD/scripts/clippy-rustc-wrapper.sh" \
    tools/typed-lints/clippy.sh "${app_args[@]}" -- -D warnings
