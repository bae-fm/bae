#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-android}"

usage() {
    echo "Usage: $0 [--release] [--abi <arm64-v8a|x86_64>]..."
    echo "  Builds bae-bridge for Android. Debug by default."
    echo "  Without --abi, builds both ABIs. run.sh passes the connected device's"
    echo "  ABI so a local install builds and ships only that one."
}

CARGO_PROFILE="debug"
CARGO_FLAGS=""
ABIS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help) usage; exit 0;;
        --release) CARGO_PROFILE="release"; CARGO_FLAGS="--release"; shift;;
        --abi) ABIS+=("$2"); shift 2;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 1;;
    esac
done
[[ ${#ABIS[@]} -eq 0 ]] && ABIS=(arm64-v8a x86_64)

NDK_HOME="${ANDROID_NDK_HOME:-/Users/dima/Library/Android/sdk/ndk/29.0.14206865}"
TOOLCHAIN="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64"
OBJCOPY="$TOOLCHAIN/bin/llvm-objcopy"
STRIP="$TOOLCHAIN/bin/llvm-strip"

# Android ABI -> Rust target triple / FFmpeg arch dir. Only arm64-v8a and x86_64
# are supported (the FFmpeg cross-build covers just those).
rust_target() { case "$1" in
    arm64-v8a) echo aarch64-linux-android;;
    x86_64)    echo x86_64-linux-android;;
    *) echo "Unsupported ABI: $1 (have arm64-v8a, x86_64)" >&2; exit 1;;
esac }
ffmpeg_arch() { case "$1" in
    arm64-v8a) echo aarch64;;
    x86_64)    echo x86_64;;
esac }

rustup target add aarch64-linux-android x86_64-linux-android 2>/dev/null || true

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/bin/aarch64-linux-android35-clang"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$TOOLCHAIN/bin/x86_64-linux-android35-clang"

export CC_aarch64_linux_android="$TOOLCHAIN/bin/aarch64-linux-android35-clang"
export AR_aarch64_linux_android="$TOOLCHAIN/bin/llvm-ar"
export CC_x86_64_linux_android="$TOOLCHAIN/bin/x86_64-linux-android35-clang"
export AR_x86_64_linux_android="$TOOLCHAIN/bin/llvm-ar"

# FFmpeg for in-core audio decode. ffmpeg-sys-next locates the libs/headers via
# FFMPEG_DIR (set per-target on each build below). In the FFMPEG_DIR path it does
# NOT forward --target/--sysroot to bindgen, so without these per-triple clang
# args bindgen emits HOST-ABI structs that compile but corrupt every FFmpeg
# struct layout at runtime. Build the libs first: scripts/build-ffmpeg-android.sh
FFMPEG_PREFIX="$(pwd)/third_party/ffmpeg-android"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--target=aarch64-linux-android35 --sysroot=$TOOLCHAIN/sysroot -I$FFMPEG_PREFIX/aarch64/include"
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android="--target=x86_64-linux-android35 --sysroot=$TOOLCHAIN/sysroot -I$FFMPEG_PREFIX/x86_64/include"

# Build each selected ABI.
for ABI in "${ABIS[@]}"; do
    TARGET=$(rust_target "$ABI")
    FA=$(ffmpeg_arch "$ABI")
    if [ ! -f "$FFMPEG_PREFIX/$FA/lib/libavcodec.so" ]; then
        echo "FFmpeg for Android ($FA) not built. Run: ./scripts/build-ffmpeg-android.sh" >&2
        exit 1
    fi
    echo "Building bae-bridge for $ABI ($TARGET, $CARGO_PROFILE)..."
    FFMPEG_DIR="$FFMPEG_PREFIX/$FA" RUSTC_WRAPPER="" cargo build $CARGO_FLAGS --target "$TARGET" -p bae-bridge
done

# Generate bindings from the built static lib, not a host build, so the Kotlin
# API matches the target exactly — mobile-only exports (e.g. setCaCertDir) are
# present and target-gated desktop ones are absent. Read the .a, not the cdylib
# .so: the release profile strips the .so (strip = true), which removes the
# uniffi metadata uniffi-bindgen needs, so the .so yields empty bindings in
# release. The .a is never stripped. uniffi metadata is arch-independent, so any
# selected target works; uniffi-bindgen reads it statically without loading the
# object. (Mirrors build-ios.sh, which reads the iOS .a.)
FIRST_TARGET=$(rust_target "${ABIS[0]}")
echo "Generating Kotlin bindings..."
mkdir -p bae-bridge/kotlin-bindings
cargo run --bin uniffi-bindgen generate \
    --library "$CARGO_TARGET_DIR/$FIRST_TARGET/$CARGO_PROFILE/libbae_bridge.a" \
    --language kotlin \
    --out-dir bae-bridge/kotlin-bindings/ \
    --no-format

# Install the .so files. Wipe both managed ABI dirs first so a stale other-ABI
# build can't ride along in the APK, then repopulate only the selected ABIs.
echo "Installing .so files (stripped; debug symbols split out)..."
JNILIBS=bae-android/app/src/main/jniLibs
rm -rf "$JNILIBS/arm64-v8a" "$JNILIBS/x86_64"
for ABI in "${ABIS[@]}"; do
    TARGET=$(rust_target "$ABI")
    FA=$(ffmpeg_arch "$ABI")
    DEST="$JNILIBS/$ABI"
    SYMDIR="bae-android/debug-symbols/$ABI"
    mkdir -p "$DEST" "$SYMDIR"
    SRC="$CARGO_TARGET_DIR/$TARGET/$CARGO_PROFILE/libbae_bridge.so"
    # Split debug info: keep full DWARF in a host-side sidecar and ship a
    # stripped .so (~44 MB vs ~380 MB unstripped). Native (Rust) crashes stay
    # symbolicatable via the sidecar:
    #   ndk-stack --sym bae-android/debug-symbols/<abi> --dump <logcat>
    # Runs for both debug and release — release ships stripped the same way.
    "$OBJCOPY" --only-keep-debug "$SRC" "$SYMDIR/libbae_bridge.so.debug"
    "$STRIP" --strip-all -o "$DEST/libbae_bridge.so" "$SRC"
    "$OBJCOPY" --add-gnu-debuglink="$SYMDIR/libbae_bridge.so.debug" "$DEST/libbae_bridge.so"
    # FFmpeg shared libs libbae_bridge.so links against — must ship in jniLibs
    # next to it (unversioned sonames). libswscale is built but not linked (no
    # video), so it's not copied.
    for so in libavcodec libavformat libavutil libswresample; do
        cp "$FFMPEG_PREFIX/$FA/lib/$so.so" "$DEST/"
    done
done

echo ""
echo "Done ($CARGO_PROFILE) for: ${ABIS[*]}"
echo "Outputs:"
for ABI in "${ABIS[@]}"; do
    echo "  $JNILIBS/$ABI/libbae_bridge.so (stripped)"
    echo "  bae-android/debug-symbols/$ABI/libbae_bridge.so.debug (symbols)"
done
echo "  bae-bridge/kotlin-bindings/"
