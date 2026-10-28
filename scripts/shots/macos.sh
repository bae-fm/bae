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
# render or an expected PNG is missing.

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
    -derivedDataPath .build/derivedData \
    test

# The test writing a PNG is not proof it landed — verify every expected scene
# produced a non-trivial file so a silently empty capture fails the script.
cd "$ROOT"
missing=0
for scene in \
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
    import-bulk-selection \
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
    storage-manager-one-sync-inspector \
    storage-manager-pinning-inspector \
    storage-manager-pin-queued-inspector \
    storage-manager-uploads-and-pinning \
    storage-manager-file-details \
    storage-manager-window
do
    selected=$(( ${#SCENES[@]} == 0 ))
    for prefix in ${SCENES[@]+"${SCENES[@]}"}; do
        [[ "$scene" == "$prefix"* ]] && selected=1
    done
    [[ "$selected" -eq 1 ]] || continue
    # Every scene renders in dark mode in each tone asked for; dark neutral
    # carries no suffix.
    if [[ ${#TONES[@]} -eq 0 ]]; then
        pngs=("$OUT_DIR/${scene}@macos.png")
    else
        pngs=()
        for tone in "${TONES[@]}"; do
            if [[ "$tone" == neutral ]]; then
                pngs+=("$OUT_DIR/${scene}@macos.png")
            else
                pngs+=("$OUT_DIR/${scene}-dark-${tone}@macos.png")
            fi
        done
    fi
    for png in "${pngs[@]}"; do
        if [[ ! -s "$png" ]]; then
            echo "Missing capture: $png" >&2
            missing=1
        fi
    done
done
if [[ "$missing" -ne 0 ]]; then
    exit 1
fi

echo "macOS scenes captured:"
ls -l "$OUT_DIR"/*@macos.png
