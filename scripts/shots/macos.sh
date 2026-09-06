#!/usr/bin/env bash
# Capture the macOS UI scenes as PNGs for the cross-platform screenshot gallery.
#
# Usage: scripts/shots/macos.sh <output-dir>
#
# Builds the Rust bridge, generates the Xcode project, and runs the `baeShots`
# test bundle, which renders each registered scene offscreen and writes
# <scene-id>@macos.png into <output-dir>. Exits non-zero if any scene fails to
# render or an expected PNG is missing.

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: scripts/shots/macos.sh <output-dir>" >&2
    exit 1
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Resolve the output directory to an absolute path before any `cd`, and create
# it so the test's write target exists.
OUT_DIR="$1"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

# The app host links FFmpeg from the repo's prebuilt dist, never a system prefix
# — put it on the dynamic-loader path so the test host launches (same setup
# scripts/check.sh uses).
if [[ -d "$ROOT/bae-ffmpeg/dist/lib" ]]; then
    export DYLD_LIBRARY_PATH="$ROOT/bae-ffmpeg/dist/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
fi

echo "Building Rust bridge (macOS)..."
./bae-bridge/build-macos.sh
./bae-bridge/install-swift-bindings.sh macos

echo "Generating Xcode project..."
cd bae-macos/bae
xcodegen

echo "Capturing scenes into $OUT_DIR ..."
# xcodebuild forwards a host environment variable named TEST_RUNNER_<NAME> to
# the test process as <NAME> (prefix stripped) — this must be exported into
# xcodebuild's environment, not passed as a build-setting argument. The capture
# test reads BAE_SHOTS_OUT.
export TEST_RUNNER_BAE_SHOTS_OUT="$OUT_DIR"
xcodebuild -project bae.xcodeproj -scheme baeShots -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath .build/derivedData \
    test

# The test writing a PNG is not proof it landed — verify every expected scene
# produced a non-trivial file so a silently empty capture fails the script.
cd "$ROOT"
missing=0
for scene in \
    import-combine-folders \
    artwork-lightbox \
    cover-picker-unlinked \
    cover-picker-wide \
    cover-picker-short \
    story-1-first-run \
    story-3-empty-library \
    import-release-queue \
    import-release-ambiguity-narrow \
    import-release-queue-collapsed \
    import-release-scanning-refresh \
    import-release-resolved-reversal \
    import-mapping-cue-wide \
    import-mapping-cue-narrow \
    queue-pane-standard \
    queue-pane-narrow \
    storage-manager-dense \
    storage-manager-empty \
    storage-manager-empty-ish \
    storage-manager-empty-ish-inspector \
    storage-manager-active-file-inspector \
    storage-manager-idle-file-inspector \
    storage-manager-one-sync-inspector
do
    png="$OUT_DIR/${scene}@macos.png"
    if [[ ! -s "$png" ]]; then
        echo "Missing capture: $png" >&2
        missing=1
    fi
done
if [[ "$missing" -ne 0 ]]; then
    exit 1
fi

echo "macOS scenes captured:"
ls -l "$OUT_DIR"/*@macos.png
