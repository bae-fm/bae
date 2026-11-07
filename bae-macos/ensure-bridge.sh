#!/bin/sh
# Builds the bridge when BaeKit's xcframework is absent. xcodebuild resolves
# the packages before the scheme's pre-action (prepare-build.sh) can build it,
# and SwiftPM refuses a binary target with no artifact. A cold checkout
# therefore needs the bridge before xcodebuild; a warm one leaves it to the
# pre-action.
#
# Usage: bae-macos/ensure-bridge.sh <Debug|Release>
# Run from anywhere; honors BAE_SKIP_RUST_BRIDGE=YES by failing, since a
# skipped build cannot produce the missing artifact.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(dirname "$SCRIPT_DIR")
XCFRAMEWORK="$REPO_ROOT/BaeKit/Frameworks/BaeBridgeFFI.xcframework"

if [ -f "$XCFRAMEWORK/Info.plist" ]; then
  exit 0
fi

if [ "${BAE_SKIP_RUST_BRIDGE:-NO}" = YES ]; then
  echo "BaeKit/Frameworks/BaeBridgeFFI.xcframework is missing and the Rust bridge build is skipped." >&2
  echo "Build it once with ./bae-bridge/build-macos.sh, or run without --skip-rust." >&2
  exit 1
fi

CONFIGURATION="${1:?usage: ensure-bridge.sh <Debug|Release>}" \
  exec "$SCRIPT_DIR/prepare-build.sh"
