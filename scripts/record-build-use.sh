#!/bin/sh
# Records the build copies a cargo report on stdin
# (--message-format=json-render-diagnostics) names, through the machine's
# `builds` tool, for a build whose report the script itself reads; any other
# build runs through scripts/cargo.sh. Without the tool this does nothing.

builds="$HOME/.local/bin/builds"
[ -x "$builds" ] && exec "$builds" record-use
exit 0
