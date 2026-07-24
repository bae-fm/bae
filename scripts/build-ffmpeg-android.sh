#!/usr/bin/env bash
#
# Download prebuilt audio-only FFmpeg for the Android NDK targets from the
# bae-ffmpeg fork's release. Every platform and build context (local dev, CI,
# release) now links the SAME prebuilt fork artifacts at the same FFmpeg version
# -- no system FFmpeg, and no per-machine cross-compile from vanilla source.
# ffmpeg-sys-next links these via FFMPEG_DIR; bae-bridge/build-android.sh points
# it at the per-arch dirs below.
#
# The fork builds SHARED libs with UNVERSIONED sonames (libavcodec.so, not
# libavcodec.so.61) so Android's packager + dynamic loader find them;
# bae-bridge/build-android.sh ships them as jniLibs sidecars next to
# libbae_bridge.so. LGPL-clean: no gpl/version3/nonfree, shared (replaceable).
#
# Output: bae-ffmpeg/android/<arch>/{lib,include} for aarch64, x86_64.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="v8.1.2-bae7"
BASE_URL="https://github.com/bae-fm/bae-ffmpeg/releases/download/$VERSION"
OUT="$REPO_ROOT/bae-ffmpeg/android"

download() {
  local arch="$1"
  local dest="$OUT/$arch"
  echo "=== downloading ffmpeg-android-$arch ($VERSION) ==="
  rm -rf "$dest"
  mkdir -p "$dest"
  curl -fL "$BASE_URL/ffmpeg-android-$arch.tar.gz" | tar xz -C "$dest"
  if [ ! -f "$dest/lib/libavcodec.so" ]; then
    echo "FATAL: libavcodec.so missing in $dest after download" >&2
    exit 1
  fi
}

download aarch64
download x86_64

echo "=== DONE. FFmpeg installed under $OUT/{aarch64,x86_64} ==="
