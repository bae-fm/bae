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
OUT="$REPO_ROOT/bae-ffmpeg/android"

# Each arch's dist ships in the APK, so it may load only its own libraries and
# Android's (install-ffmpeg-dist.sh's android check). An arch that already
# holds the pinned release is left as it is.
for arch in aarch64 x86_64; do
  "$REPO_ROOT/scripts/install-ffmpeg-dist.sh" "ffmpeg-android-$arch.tar.gz" "$OUT/$arch" android
done
