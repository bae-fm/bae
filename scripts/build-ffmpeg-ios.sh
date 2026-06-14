#!/usr/bin/env bash
#
# Cross-compile FFmpeg for the iOS targets so bae-core can decode audio in-core
# on iOS (mirrors scripts/build-ffmpeg-android.sh). iOS forbids shipping custom
# dynamic libs, so these are STATIC archives (.a) that link into libbae_bridge.a
# inside the xcframework — there is no jniLibs sidecar like Android. ffmpeg-sys-next
# links them via FFMPEG_DIR; bae-bridge/build-ios.sh points it here.
#
# Output: third_party/ffmpeg-ios/<arch>/{lib,include}. Two arches, never lipo'd
# together (device and simulator are distinct platforms in the xcframework):
#   aarch64-apple-ios       (device,    iphoneos SDK)
#   aarch64-apple-ios-sim   (simulator, iphonesimulator SDK)
#
# Re-runnable. Reuses the FFmpeg source cloned by build-ffmpeg-android.sh.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IOS_MIN="16.0"
FF_TAG="n7.1" # matches ffmpeg-sys-next 8.x (FFmpeg 7.1)

SRC="$REPO_ROOT/third_party/ffmpeg-src"
OUT="$REPO_ROOT/third_party/ffmpeg-ios"

# --- FFmpeg source (shared with the Android build) ---
if [ ! -d "$SRC/.git" ]; then
  echo "=== cloning FFmpeg $FF_TAG ==="
  git clone --depth 1 --branch "$FF_TAG" https://github.com/FFmpeg/FFmpeg.git "$SRC"
fi

# Same audio components as Android (audio_codec.rs is identical across platforms).
DECODERS="flac,mp3,ape,alac,aac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8,pcm_s16be,pcm_s24be,pcm_s32be"
ENCODERS="flac"
MUXERS="flac"
DEMUXERS="flac,mp3,ape,mov,ipod,ogg,wav,aiff"
PARSERS="flac,mpegaudio,aac"

build_arch() {
  local ARCH="$1" SDK="$2" MIN_FLAG="$3" TARGET_TRIPLE="$4"
  local PREFIX="$OUT/$ARCH"
  local BUILD="$SRC/build-$ARCH"
  local SYSROOT; SYSROOT="$(xcrun --sdk "$SDK" --show-sdk-path)"
  local CLANG; CLANG="$(xcrun --sdk "$SDK" -f clang)"
  # `-target` marks simulator vs device for the integrated assembler/linker; the
  # min-version flag pins the deployment floor.
  local CFLAGS="-arch arm64 -isysroot $SYSROOT $MIN_FLAG"
  [ -n "$TARGET_TRIPLE" ] && CFLAGS="$CFLAGS -target $TARGET_TRIPLE"

  echo "=== configuring FFmpeg for $ARCH ($SDK) ==="
  rm -rf "$BUILD"; mkdir -p "$BUILD"
  ( cd "$BUILD" && "$SRC/configure" \
      --prefix="$PREFIX" \
      --enable-cross-compile --target-os=darwin --arch=arm64 \
      --sysroot="$SYSROOT" \
      --cc="$CLANG" \
      --extra-cflags="$CFLAGS" \
      --extra-ldflags="$CFLAGS" \
      --enable-pic --enable-static --disable-shared \
      --disable-programs --disable-doc \
      --disable-avdevice --disable-avfilter --disable-postproc --disable-network \
      --enable-swresample \
      --disable-everything \
      --enable-decoder="$DECODERS" \
      --enable-encoder="$ENCODERS" \
      --enable-muxer="$MUXERS" \
      --enable-demuxer="$DEMUXERS" \
      --enable-parser="$PARSERS" )
  echo "=== building FFmpeg for $ARCH ==="
  make -C "$BUILD" -j"$(sysctl -n hw.ncpu)"
  make -C "$BUILD" install
  echo "=== $ARCH static libs ==="
  ls -la "$PREFIX"/lib/*.a
}

# Device: arm64 iphoneos. Apple arm64 uses clang's integrated assembler (no nasm).
build_arch "aarch64-apple-ios" "iphoneos" "-mios-version-min=$IOS_MIN" ""
# Simulator: arm64 iphonesimulator — the -target triple is what distinguishes it.
build_arch "aarch64-apple-ios-sim" "iphonesimulator" "-mios-simulator-version-min=$IOS_MIN" "arm64-apple-ios${IOS_MIN}-simulator"

echo "=== DONE. FFmpeg installed under $OUT/{aarch64-apple-ios,aarch64-apple-ios-sim} ==="
