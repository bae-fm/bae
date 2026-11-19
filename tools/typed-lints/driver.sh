#!/bin/sh
# Points tools/typed-lints/built at the typed lints' driver for this
# checkout's toolchain and coven revision (tool.sh). With the `builds` tool,
# `builds tool` keeps one driver per key on each machine, copying it from
# another machine that has it or building it; when a commit hook's check runs
# it, `builds tool` only points the link at the driver the machine has, and
# fails at once, naming `builds setup`, when there is none. Without the tool
# (CI), the driver builds in this checkout's target folder.
set -eu
cd "$(dirname "$0")/../.."
builds="$HOME/.local/bin/builds"
if [ -x "$builds" ]; then
    exec "$builds" tool typed-lints >/dev/null
fi
# The build there is kept while its key is the checkout's, since CI runs
# clippy several times in one job.
out="$PWD/target/typed-lints/built"
key=$(tools/typed-lints/tool.sh key)
if [ "$(cat "$out/key" 2>/dev/null)" != "$key" ]; then
    rm -rf "$out"
    # A caller's CARGO_TARGET_DIR (CI's iOS clippy) would move the driver's
    # build out of the folder tool.sh copies it from.
    env -u CARGO_TARGET_DIR BUILDS_TOOL_WORK="$PWD/target/typed-lints/work" \
        tools/typed-lints/tool.sh build "$out"
    printf '%s\n' "$key" > "$out/key"
fi
"$out/check"
rm -rf tools/typed-lints/built
ln -s "$out" tools/typed-lints/built
