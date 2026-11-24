#!/usr/bin/env bash
# Build libvips for the Android ABIs the app ships, into
# bae-libvips/android/<arch>/{lib,include}, as scripts/build-ffmpeg-android.sh
# installs FFmpeg's. An arch that already holds the pinned release is left as
# it is.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$REPO_ROOT/scripts/prepare-libvips.sh"
OUT="$(cd "$REPO_ROOT/bae-libvips" && pwd -P)/android"
for arch in aarch64 x86_64; do
    "$REPO_ROOT/scripts/build-libvips.sh" "android-$arch" "$OUT/$arch"
done
