#!/bin/sh
set -eu

# Scheme pre-actions inherit Xcode settings for every Apple platform. Cargo
# build scripts treat several of those names as compiler inputs, so run the
# bridge build with only the host settings it actually needs.
#
# The FFmpeg variables must survive the scrub when the environment carries
# them: CI stages the dist outside the repo (/opt/bae-ffmpeg) and exports
# FFMPEG_DIR & co., while local checkouts rely on .cargo/config.toml's
# relative bae-ffmpeg/dist. Dropping them here changes ffmpeg-sys' build
# fingerprint between the workflow's direct bridge build and this pre-action,
# forcing a rebuild in an environment that can no longer find the headers.
if [ "${BAE_MACOS_PREPARE_CLEAN_ENV:-0}" != 1 ]; then
  set -- \
    HOME="$HOME" \
    USER="${USER:-}" \
    PATH="$PATH" \
    TMPDIR="${TMPDIR:-/tmp}" \
    DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}" \
    MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-15.0}" \
    BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES:-oauth-providers,cloudkit,desktop}" \
    BAE_MACOS_PREPARE_CLEAN_ENV=1
  for var in FFMPEG_DIR PKG_CONFIG_PATH LIBRARY_PATH DYLD_LIBRARY_PATH \
    BINDGEN_EXTRA_CLANG_ARGS C_INCLUDE_PATH; do
    eval "val=\${$var:-}"
    if [ -n "$val" ]; then
      set -- "$@" "$var=$val"
    fi
  done
  exec env -i "$@" "$0"
fi

# Xcode launched via the Dock does not source the interactive shell profile.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(dirname "$SCRIPT_DIR")
cd "$REPO_ROOT"

./bae-bridge/build-macos.sh
