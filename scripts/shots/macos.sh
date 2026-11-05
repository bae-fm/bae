#!/usr/bin/env bash
# Capture the macOS UI scenes as PNGs for the cross-platform screenshot gallery.
#
# Usage: scripts/shots/macos.sh <output-dir> [--scene <id-prefix>]... [--tone <tone>]...
#
# --scene renders only the scenes whose id starts with one of the prefixes, and
# --tone only those surface tones (neutral, slate, plum, ...); both modes always
# render. Without them every scene renders in every tone, as the gallery needs.
#
# Builds the Rust bridge, generates the Xcode project, and runs the `baeShots`
# test bundle, which renders each registered scene offscreen and writes
# <scene-id>@macos.png into <output-dir>. Exits non-zero if any scene fails to
# render, encode or write: the capture test fails the run.

set -euo pipefail

usage() {
    echo "Usage: scripts/shots/macos.sh <output-dir> [--scene <id-prefix>]... [--tone <tone>]..." >&2
    exit 1
}
[[ $# -ge 1 ]] || usage
OUT_DIR="$1"
shift
SCENES=()
TONES=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --scene) [[ $# -ge 2 ]] || usage; SCENES+=("$2"); shift 2 ;;
        --tone) [[ $# -ge 2 ]] || usage; TONES+=("$2"); shift 2 ;;
        *) usage ;;
    esac
done

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
./scripts/build-layout.sh
source scripts/xcode-trees.sh

# Resolve the output directory to an absolute path before any `cd`, and create
# it so the test's write target exists.
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
TEST_RUNNER_BAE_SHOTS_SCENES="$(IFS=,; echo "${SCENES[*]-}")"
TEST_RUNNER_BAE_SHOTS_TONES="$(IFS=,; echo "${TONES[*]-}")"
export TEST_RUNNER_BAE_SHOTS_SCENES TEST_RUNNER_BAE_SHOTS_TONES
xcodebuild -project bae.xcodeproj -scheme baeShots -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    "${XCODE_MACOS_UNSIGNED_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" \
    test

echo "macOS scenes captured:"
ls -l "$OUT_DIR"/*@macos.png
