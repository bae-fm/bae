#!/usr/bin/env bash
#
# Download prebuilt audio-only FFmpeg for the iOS targets from the bae-ffmpeg
# fork's release. Every platform and build context (local dev, CI, release) now
# links the SAME prebuilt fork artifacts at the same FFmpeg version -- no system
# FFmpeg, and no per-machine cross-compile from vanilla source. ffmpeg-sys-next
# links these via FFMPEG_DIR; bae-bridge/build-ios.sh points it at the per-arch
# dirs below.
#
# iOS forbids shipping custom dynamic libs, so the fork builds STATIC archives
# (.a) for iOS; bae-bridge/build-ios.sh merges them into libbae_bridge.a inside
# the xcframework. Device and simulator are distinct platforms in the
# xcframework and are never lipo'd together.
#
# Output: bae-ffmpeg/ios/<arch>/{lib,include}
#   aarch64-apple-ios       (device,    fork label ios-arm64)
#   aarch64-apple-ios-sim   (simulator, fork label ios-sim-arm64)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO_ROOT/bae-ffmpeg/ios"

# Fork release label -> bae per-target dir. A dir that already holds the
# pinned release is left as it is.
"$REPO_ROOT/scripts/install-ffmpeg-dist.sh" ffmpeg-ios-arm64.tar.gz "$OUT/aarch64-apple-ios" ios
"$REPO_ROOT/scripts/install-ffmpeg-dist.sh" ffmpeg-ios-sim-arm64.tar.gz "$OUT/aarch64-apple-ios-sim" ios
