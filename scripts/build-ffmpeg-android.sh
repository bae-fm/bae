#!/usr/bin/env bash
#
# Cross-compile FFmpeg for the Android NDK targets so bae-core can decode audio
# in-process (the in-core decode half of the dual-mode player). ffmpeg-sys-next
# links these via the FFMPEG_DIR discovery path; build-android.sh points it here.
#
# Output: third_party/ffmpeg-android/<arch>/{lib,include}, shared libs with
# UNVERSIONED sonames (libavcodec.so, not libavcodec.so.61) so Android's
# packager + dynamic loader find them. LGPL-clean: no --enable-gpl/version3/
# nonfree, shared libs (replaceable). Only the decoders/demuxers audio_codec.rs
# actually touches are enabled.
#
# Re-runnable. FFmpeg source is cloned once into third_party/ffmpeg-src.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NDK="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/29.0.14206865}"
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/darwin-x86_64"
API=26 # matches minSdk in bae-android/app/build.gradle.kts
FF_TAG="n7.1" # matches ffmpeg-sys-next 8.x (FFmpeg 7.1)

SRC="$REPO_ROOT/third_party/ffmpeg-src"
OUT="$REPO_ROOT/third_party/ffmpeg-android"

if [ ! -x "$TOOLCHAIN/bin/llvm-ar" ]; then
  echo "FATAL: NDK toolchain not found at $TOOLCHAIN (set ANDROID_NDK_HOME)" >&2
  exit 1
fi

# --- FFmpeg source ---
if [ ! -d "$SRC/.git" ]; then
  echo "=== cloning FFmpeg $FF_TAG ==="
  git clone --depth 1 --branch "$FF_TAG" https://github.com/FFmpeg/FFmpeg.git "$SRC"
fi

# Audio components bae-core needs (audio_codec.rs): FLAC/MP3/APE/ALAC/AAC decode,
# FLAC encode+mux (synthetic standalone-FLAC for CUE byte-range tracks),
# swresample, custom AVIO (no protocols/network).
DECODERS="flac,mp3,ape,alac,aac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8,pcm_s16be,pcm_s24be,pcm_s32be"
ENCODERS="flac"
MUXERS="flac"
DEMUXERS="flac,mp3,ape,mov,ipod,ogg,wav,aiff"
PARSERS="flac,mpegaudio,aac"

build_arch() {
  local ARCH="$1" FFARCH="$2" TRIPLE="$3" EXTRA="$4"
  local PREFIX="$OUT/$ARCH"
  local BUILD="$SRC/build-$ARCH"
  echo "=== configuring FFmpeg for $ARCH ($TRIPLE) ==="
  rm -rf "$BUILD"; mkdir -p "$BUILD"
  ( cd "$BUILD" && "$SRC/configure" \
      --prefix="$PREFIX" \
      --enable-cross-compile --target-os=android --arch="$FFARCH" \
      --sysroot="$TOOLCHAIN/sysroot" \
      --cc="$TOOLCHAIN/bin/${TRIPLE}${API}-clang" \
      --cxx="$TOOLCHAIN/bin/${TRIPLE}${API}-clang++" \
      --cross-prefix="$TOOLCHAIN/bin/llvm-" \
      --nm="$TOOLCHAIN/bin/llvm-nm" --ar="$TOOLCHAIN/bin/llvm-ar" \
      --ranlib="$TOOLCHAIN/bin/llvm-ranlib" --strip="$TOOLCHAIN/bin/llvm-strip" \
      --enable-pic --enable-shared --disable-static \
      --disable-programs --disable-doc \
      --disable-avdevice --disable-avfilter --disable-postproc --disable-network \
      --enable-swresample \
      --disable-everything \
      --enable-decoder="$DECODERS" \
      --enable-encoder="$ENCODERS" \
      --enable-muxer="$MUXERS" \
      --enable-demuxer="$DEMUXERS" \
      --enable-parser="$PARSERS" \
      --extra-ldflags="-Wl,-z,max-page-size=16384" \
      $EXTRA )
  echo "=== building FFmpeg for $ARCH ==="
  # Override the soname/install vars so output is libavcodec.so with SONAME
  # libavcodec.so (no version suffix) — required for Android's loader.
  make -C "$BUILD" -j"$(sysctl -n hw.ncpu)" \
    SLIBNAME_WITH_VERSION='$(SLIBNAME)' \
    SLIBNAME_WITH_MAJOR='$(SLIBNAME)' \
    SLIB_INSTALL_NAME='$(SLIBNAME)' \
    SLIB_INSTALL_LINKS=''
  make -C "$BUILD" install \
    SLIBNAME_WITH_VERSION='$(SLIBNAME)' \
    SLIBNAME_WITH_MAJOR='$(SLIBNAME)' \
    SLIB_INSTALL_NAME='$(SLIBNAME)' \
    SLIB_INSTALL_LINKS=''
  echo "=== $ARCH sonames ==="
  for so in "$PREFIX"/lib/lib*.so; do
    printf '  %s -> SONAME ' "$(basename "$so")"
    "$TOOLCHAIN/bin/llvm-readelf" -d "$so" | grep SONAME || echo "(none)"
  done
}

# arm64: real devices, clang/NEON (no external assembler).
build_arch "aarch64" "aarch64" "aarch64-linux-android" ""
# x86_64: emulator only — disable x86 asm so we don't need nasm on the host.
build_arch "x86_64" "x86_64" "x86_64-linux-android" "--disable-x86asm"

echo "=== DONE. FFmpeg installed under $OUT/{aarch64,x86_64} ==="
