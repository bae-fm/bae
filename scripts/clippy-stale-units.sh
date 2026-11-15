#!/bin/sh
# Makes the units of the crates named as arguments stale, so cargo rebuilds
# and lints them. A crate scripts/clippy-rustc-wrapper.sh compiled without
# lints is fresh to cargo, which would then never lint it. Its records name the
# unit's folder in .fingerprint; dating that folder's files to 1970 makes cargo
# see every input of the unit as newer than its last build, so it rebuilds, and
# lints, each unit of a crate named here.
#
# scripts/clippy-crates.sh names this script in BUILDS_CARGO_BEFORE, so it runs
# in the checkout before cargo on whichever machine runs cargo: the folders it
# changes are that machine's `target`.
set -eu
cd "$(dirname "$0")/.."

for crate in "$@"; do
    for record in target/*/clippy-unlinted/"$crate"__*; do
        [ -f "$record" ] || continue
        fingerprint=$(cat "$record")
        if [ -d "$fingerprint" ]; then
            find "$fingerprint" -type f -exec touch -t 197001020000 {} +
        fi
        rm -f "$record"
    done
done
