#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-ios}"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for iOS (device + simulator). Debug by default."
    exit 0
fi

CARGO_PROFILE="debug"
CARGO_FLAGS=""
if [[ "${1:-}" == "--release" ]]; then
    CARGO_PROFILE="release"
    CARGO_FLAGS="--release"
fi

if command -v sccache &> /dev/null; then
    export RUSTC_WRAPPER=sccache
fi

rustup target add aarch64-apple-ios aarch64-apple-ios-sim 2>/dev/null || true

# iOS minimum deployment target. Must be >= the prebuilt FFmpeg static libs'
# minos (16.0); a lower target makes the linker reach for runtime symbols
# (`___chkstk_darwin`) that the SDK only vends for the negotiated minimum.
export IPHONEOS_DEPLOYMENT_TARGET=16.0

# FFmpeg for in-core audio decode. ffmpeg-sys-next locates the libs/headers via
# FFMPEG_DIR (set per-arch on each build below) and emits the -lavcodec/-lavformat
# link flags from it. In the FFMPEG_DIR path it does NOT forward --target/-isysroot
# to bindgen, so without these per-arch clang args bindgen emits HOST-ABI structs
# that compile but corrupt every FFmpeg struct layout at runtime. The libs are
# STATIC .a's built by scripts/build-ffmpeg-ios.sh.
FFMPEG_DEVICE="$(pwd)/third_party/ffmpeg-ios/aarch64-apple-ios"
FFMPEG_SIM="$(pwd)/third_party/ffmpeg-ios/aarch64-apple-ios-sim"
if [ ! -f "$FFMPEG_DEVICE/lib/libavcodec.a" ]; then
    echo "FFmpeg for iOS not built. Run: ./scripts/build-ffmpeg-ios.sh" >&2
    exit 1
fi

DEVICE_SDK="$(xcrun --sdk iphoneos --show-sdk-path)"
SIM_SDK="$(xcrun --sdk iphonesimulator --show-sdk-path)"

# FFmpeg pulls zlib (`uncompress`); the SDK vends it as -lz. The cdylib link step
# (crate-type includes cdylib) needs it spelled out — the staticlib we ship
# defers it to the consuming app, but the cargo build links the cdylib too.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-lz"

echo "Building for iOS device (arm64, $CARGO_PROFILE)..."
FFMPEG_DIR="$FFMPEG_DEVICE" \
BINDGEN_EXTRA_CLANG_ARGS="--target=arm64-apple-ios16.0 -isysroot $DEVICE_SDK -I$FFMPEG_DEVICE/include" \
cargo build $CARGO_FLAGS --target aarch64-apple-ios -p bae-bridge --features oauth-providers,cloudkit

echo "Building for iOS simulator (arm64, $CARGO_PROFILE)..."
FFMPEG_DIR="$FFMPEG_SIM" \
SDKROOT="$SIM_SDK" \
BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$SIM_SDK -target arm64-apple-ios16.0-simulator -I$FFMPEG_SIM/include" \
cargo build $CARGO_FLAGS --target aarch64-apple-ios-sim -p bae-bridge --features oauth-providers,cloudkit

echo "Generating Swift bindings..."
mkdir -p bae-bridge/swift-bindings
cargo run --bin uniffi-bindgen generate \
    --library "$CARGO_TARGET_DIR/aarch64-apple-ios/$CARGO_PROFILE/libbae_bridge.a" \
    --language swift \
    --out-dir bae-bridge/swift-bindings/

# Copy the iOS-flavored bindings into the iOS app source tree. The shared
# `bae-bridge/swift-bindings/` dir is scratch that both this script and
# build-macos.sh regenerate (with different cargo features — iOS is
# cloudkit-only, macOS pulls in the desktop import/cd methods). The iOS
# app must compile against the cloudkit-only bindings whose checksum symbols the
# iOS xcframework actually exports, so it reads from its own copy here rather
# than the shared dir a later macOS build would clobber. Gitignored, mirroring
# the macOS `bae-macos/bae/bae/bae_bridge.swift` copy.
cp bae-bridge/swift-bindings/bae_bridge.swift bae-ios/bae/bae/bae_bridge.swift

# Merge the FFmpeg static libs into the bridge staticlib per-arch. A Rust
# staticlib (libbae_bridge.a) does NOT bundle its C dependencies — it only
# carries Rust objects plus the `cargo:rustc-link-lib` directives, which the
# consuming app would otherwise have to satisfy by linking libav*.a itself.
# Merging makes the xcframework self-contained: the app links only it.
# libswscale is built but not linked (no video), so it's left out.
echo "Merging FFmpeg static libs into the bridge lib..."
DEVICE_MERGED="$CARGO_TARGET_DIR/aarch64-apple-ios/$CARGO_PROFILE/libbae_bridge_merged.a"
SIM_MERGED="$CARGO_TARGET_DIR/aarch64-apple-ios-sim/$CARGO_PROFILE/libbae_bridge_merged.a"
libtool -static -o "$DEVICE_MERGED" \
    "$CARGO_TARGET_DIR/aarch64-apple-ios/$CARGO_PROFILE/libbae_bridge.a" \
    "$FFMPEG_DEVICE/lib/libavcodec.a" \
    "$FFMPEG_DEVICE/lib/libavformat.a" \
    "$FFMPEG_DEVICE/lib/libavutil.a" \
    "$FFMPEG_DEVICE/lib/libswresample.a"
libtool -static -o "$SIM_MERGED" \
    "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/$CARGO_PROFILE/libbae_bridge.a" \
    "$FFMPEG_SIM/lib/libavcodec.a" \
    "$FFMPEG_SIM/lib/libavformat.a" \
    "$FFMPEG_SIM/lib/libavutil.a" \
    "$FFMPEG_SIM/lib/libswresample.a"

echo "Creating iOS XCFramework..."
rm -rf bae-ios/BaeBridgeFFI-ios.xcframework

mkdir -p bae-bridge/swift-bindings/headers
cp bae-bridge/swift-bindings/bae_bridgeFFI.h bae-bridge/swift-bindings/headers/
cp bae-bridge/swift-bindings/bae_bridgeFFI.modulemap bae-bridge/swift-bindings/headers/module.modulemap

xcodebuild -create-xcframework \
    -library "$DEVICE_MERGED" \
    -headers bae-bridge/swift-bindings/headers \
    -library "$SIM_MERGED" \
    -headers bae-bridge/swift-bindings/headers \
    -output bae-ios/BaeBridgeFFI-ios.xcframework

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  bae-ios/BaeBridgeFFI-ios.xcframework/"
echo "  bae-bridge/swift-bindings/bae_bridge.swift"
