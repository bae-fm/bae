#!/bin/sh
set -eu

# Scheme pre-actions inherit Xcode settings for every Apple platform. Cargo
# build scripts treat several of those names as compiler inputs, so run the
# bridge build with only the host settings it actually needs.
if [ "${BAE_MACOS_PREPARE_CLEAN_ENV:-0}" != 1 ]; then
  exec env -i \
    HOME="$HOME" \
    USER="${USER:-}" \
    PATH="$PATH" \
    TMPDIR="${TMPDIR:-/tmp}" \
    DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}" \
    MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-15.0}" \
    BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES:-oauth-providers,cloudkit,desktop}" \
    BAE_MACOS_PREPARE_CLEAN_ENV=1 \
    "$0"
fi

# Xcode launched via the Dock does not source the interactive shell profile.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(dirname "$SCRIPT_DIR")
cd "$REPO_ROOT"

./bae-bridge/build-macos.sh
