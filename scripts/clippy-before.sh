#!/bin/sh
# What scripts/clippy-crates.sh needs on the machine that runs cargo, before
# cargo starts: tools/typed-lints/built pointing at the typed lints' driver
# there (tools/typed-lints/driver.sh), and the units of the crates named as
# arguments made stale (scripts/clippy-stale-units.sh). clippy-crates.sh names
# this in BUILDS_CARGO_BEFORE.
set -eu
cd "$(dirname "$0")/.."
tools/typed-lints/driver.sh
scripts/clippy-stale-units.sh "$@"
