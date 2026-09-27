#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

# This script reads the staticlib it builds back out of the target dir, so it
# owns that dir: a shared one can be rewritten by another checkout in between.
export CARGO_TARGET_DIR="target-android"

usage() {
    echo "Usage: $0 [--release] [--abi <arm64-v8a|x86_64>]..."
    echo "  Builds bae-bridge for Android. Debug by default."
    echo "  Without --abi, builds both ABIs. run.sh passes the connected device's"
    echo "  ABI so a local install builds and ships only that one."
    echo ""
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers,cast'), which picks the Gradle edition the bindings"
    echo "  feed: a set containing oauth-providers writes the 'full' edition's"
    echo "  bindings, anything else writes the 'baeium' (S3-only) edition's."
    echo "  Casting ships in every edition. For baeium:"
    echo "  BAE_BRIDGE_FEATURES=cast $0"
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

# The bridge's cargo features, which pick the Gradle edition: oauth-providers
# is 'full', without it 'baeium'. `cast` ships in every edition.
BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES-oauth-providers,cast}"

# The edition's bindings dir, which its flavor's sourceSet reads.
case ",$BAE_BRIDGE_FEATURES," in
    *,oauth-providers,*) BINDINGS_DIR="bae-bridge/kotlin-bindings-full" ;;
    *) BINDINGS_DIR="bae-bridge/kotlin-bindings-baeium" ;;
esac

NDK_HOME="${ANDROID_NDK_HOME:-/Users/dima/Library/Android/sdk/ndk/29.0.14206865}"
# NDK toolchains are named by host.
case "$(uname -s)" in
    Darwin) NDK_HOST_TAG="darwin-x86_64" ;;
    Linux) NDK_HOST_TAG="linux-x86_64" ;;
    *) echo "build-android.sh: unsupported host OS $(uname -s)" >&2; exit 1 ;;
esac
TOOLCHAIN="$NDK_HOME/toolchains/llvm/prebuilt/$NDK_HOST_TAG"
OBJCOPY="$TOOLCHAIN/bin/llvm-objcopy"
STRIP="$TOOLCHAIN/bin/llvm-strip"

# Android ABI -> Rust target triple / FFmpeg arch dir; the FFmpeg build covers
# only these two.
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

# ffmpeg-sys-next finds FFmpeg through FFMPEG_DIR and then does not pass the
# target to bindgen, so these per-triple clang args are what keep its struct
# layouts from being the host's. Build the libs first: scripts/build-ffmpeg-android.sh
FFMPEG_PREFIX="$(pwd)/bae-ffmpeg/android"
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
    echo "Building bae-bridge for $ABI ($TARGET, $CARGO_PROFILE, features: ${BAE_BRIDGE_FEATURES:-(none)})..."
    FFMPEG_DIR="$FFMPEG_PREFIX/$FA" RUSTC_WRAPPER="" cargo build $CARGO_FLAGS --target "$TARGET" -p bae-bridge --features "$BAE_BRIDGE_FEATURES"
done

# Bindings come from the built .a: the release .so is stripped of the uniffi
# metadata the generator reads.
FIRST_TARGET=$(rust_target "${ABIS[0]}")
echo "Generating Kotlin bindings into $BINDINGS_DIR ..."
# Regenerated from scratch so another feature set's functions can't linger.
rm -rf "$BINDINGS_DIR"
mkdir -p "$BINDINGS_DIR"
# The generator is its own host-only package so this build doesn't compile
# bae-core again.
cargo build -p bae-uniffi-bindgen
BINDGEN="$CARGO_TARGET_DIR/debug/uniffi-bindgen"
"$BINDGEN" generate \
    --library "$CARGO_TARGET_DIR/$FIRST_TARGET/$CARGO_PROFILE/libbae_bridge.a" \
    --language kotlin \
    --out-dir "$BINDINGS_DIR/" \
    --no-format

# loc-gen emits core_strings.xml per locale; the old set is deleted first so a
# dropped locale can't linger.
echo "Generating localization string resources (Android)..."
RES_DIR=bae-android/app/src/main/res
LOC_STAGING="$(mktemp -d)"
trap 'rm -rf "$LOC_STAGING"' EXIT
cargo run -q -p bae-loc --bin loc-gen -- emit --target android --out-dir "$LOC_STAGING"
find "$RES_DIR" -name core_strings.xml -delete
( cd "$LOC_STAGING" && find . -name core_strings.xml -print0 ) | while IFS= read -r -d '' rel; do
    dest="$RES_DIR/${rel#./}"
    mkdir -p "$(dirname "$dest")"
    cp "$LOC_STAGING/${rel#./}" "$dest"
done

echo "Generating the theme (Android)..."
cargo run -q -p bae-theme --bin theme-gen -- emit --target android --out-dir bae-android/app/generated/theme

# Both managed ABI dirs are wiped so only the selected ABIs ship.
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
    # Ship a stripped .so and keep the debug info beside it for symbolicating
    # native crashes: ndk-stack --sym bae-android/debug-symbols/<abi> --dump <logcat>
    "$OBJCOPY" --only-keep-debug "$SRC" "$SYMDIR/libbae_bridge.so.debug"
    "$STRIP" --strip-all -o "$DEST/libbae_bridge.so" "$SRC"
    "$OBJCOPY" --add-gnu-debuglink="$SYMDIR/libbae_bridge.so.debug" "$DEST/libbae_bridge.so"
    # The FFmpeg libraries libbae_bridge.so links against ship beside it.
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
echo "  $BINDINGS_DIR/"
