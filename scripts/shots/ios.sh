#!/usr/bin/env bash
# Capture the iOS UI scenes as PNGs for the cross-platform screenshot gallery.
#
# Usage: scripts/shots/ios.sh <output-dir>
#
# Builds the Rust bridge, generates the Xcode project, and runs the `baeShots`
# test bundle on a booted simulator. The test renders each scene offscreen and
# writes <scene-id>@ios.png into the app's Documents container (the simulator
# sandbox can't write to a host path); this script copies them out to
# <output-dir>. Exits non-zero if any scene fails to render or an expected PNG
# is missing.
#
# Override the simulator with BAE_SHOTS_IOS_DEVICE (a name or UDID); otherwise
# the first available iPhone is used.

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: scripts/shots/ios.sh <output-dir>" >&2
    exit 1
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

OUT_DIR="$1"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

BUNDLE_ID="fm.bae.bae"

# Pick the simulator: an explicit override, else the first available iPhone.
DEVICE="${BAE_SHOTS_IOS_DEVICE:-}"
if [[ -z "$DEVICE" ]]; then
    DEVICE="$(xcrun simctl list devices available \
        | grep -oE 'iPhone [^(]+\([0-9A-F-]{36}\)' \
        | grep -oE '[0-9A-F-]{36}' \
        | head -1)"
fi
if [[ -z "$DEVICE" ]]; then
    echo "No available iPhone simulator found." >&2
    exit 1
fi
echo "Using simulator: $DEVICE"
xcrun simctl boot "$DEVICE" 2>/dev/null || true

echo "Building Rust bridge (iOS)..."
BAE_BRIDGE_FEATURES="oauth-providers,cloudkit" ./bae-bridge/build-ios.sh
./bae-bridge/install-swift-bindings.sh ios

echo "Generating Xcode project..."
cd bae-ios/bae
xcodegen

echo "Capturing scenes on $DEVICE ..."
xcodebuild -project bae.xcodeproj -scheme baeShots -configuration Debug \
    -sdk iphonesimulator \
    -destination "id=$DEVICE" \
    -derivedDataPath .build/derivedData \
    test

# The test wrote the PNGs into the app's sandboxed Documents; copy them out.
cd "$ROOT"
CONTAINER="$(xcrun simctl get_app_container "$DEVICE" "$BUNDLE_ID" data)"
SHOTS_DIR="$CONTAINER/Documents/shots"
if [[ ! -d "$SHOTS_DIR" ]]; then
    echo "Capture directory not found in app container: $SHOTS_DIR" >&2
    exit 1
fi
cp "$SHOTS_DIR"/*@ios.png "$OUT_DIR"/

# Verify every expected scene produced a non-trivial file.
missing=0
for scene in welcome library-grid album-detail; do
    png="$OUT_DIR/${scene}@ios.png"
    if [[ ! -s "$png" ]]; then
        echo "Missing capture: $png" >&2
        missing=1
    fi
done
if [[ "$missing" -ne 0 ]]; then
    exit 1
fi

echo "iOS scenes captured:"
ls -l "$OUT_DIR"/*@ios.png
