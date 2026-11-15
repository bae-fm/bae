#!/bin/sh
# Runs cargo with these arguments. On a machine with the `builds` tool it runs
# through `builds cargo`, which records the build copies (one per crate,
# profile and dependency set) cargo used, so `builds setup` keeps those while
# it deletes the ones no build has used for days; a program's or the tests'
# output passes through. Without the tool (CI, another machine) this is cargo
# itself.

# BUILDS_CARGO_BEFORE names a program of the checkout, and its arguments, to run
# in the checkout before cargo. `builds cargo` runs it on the machine that runs
# cargo; without the tool it runs here.
builds="$HOME/.local/bin/builds"
[ -x "$builds" ] && exec "$builds" cargo "$@"
# shellcheck disable=SC2086
[ -z "${BUILDS_CARGO_BEFORE-}" ] || ${BUILDS_CARGO_BEFORE} || exit $?
exec cargo "$@"
