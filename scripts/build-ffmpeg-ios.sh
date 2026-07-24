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
VERSION="v8.1.2-bae7"
BASE_URL="https://github.com/bae-fm/bae-ffmpeg/releases/download/$VERSION"
OUT="$REPO_ROOT/bae-ffmpeg/ios"

# fork release tarball label -> bae per-target dir.
download() {
  local label="$1" dir="$2"
  local dest="$OUT/$dir"
  echo "=== downloading ffmpeg-$label ($VERSION) ==="
  rm -rf "$dest"
  mkdir -p "$dest"
  curl -fL "$BASE_URL/ffmpeg-$label.tar.gz" | tar xz -C "$dest"
  if [ ! -f "$dest/lib/libavcodec.a" ]; then
    echo "FATAL: libavcodec.a missing in $dest after download" >&2
    exit 1
  fi
}

download ios-arm64     aarch64-apple-ios
download ios-sim-arm64 aarch64-apple-ios-sim

echo "=== DONE. FFmpeg installed under $OUT/{aarch64-apple-ios,aarch64-apple-ios-sim} ==="
