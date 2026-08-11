#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

# This script compiles the bridge and then reads the resulting staticlib back
# out of the target dir to generate bindings from it, so it has to own the
# directory it reads from. An inherited CARGO_TARGET_DIR can be shared with
# other checkouts, and a concurrent build there rewrites the same artifact from
# different sources between the build and the read — the generators then emit
# bindings for a bridge nobody asked for, and it surfaces later as a Swift or
# C# compile error nowhere near its cause. Local and unconditional on purpose.
export CARGO_TARGET_DIR="target-ios"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for iOS (device + simulator). Debug by default."
    echo ""
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers,cloudkit,cast'; iOS has no 'desktop'). The Swift"
    echo "  compilation conditions are derived from the same set so the generated"
    echo "  bindings and the app's #if guards never disagree. For a baeium"
    echo "  (S3-only) build: BAE_BRIDGE_FEATURES=cast $0"
    exit 0
fi

CARGO_PROFILE="debug"
CARGO_FLAGS=""
if [[ "${1:-}" == "--release" ]]; then
    CARGO_PROFILE="release"
    CARGO_FLAGS="--release"
fi

# The cargo feature set the bridge compiles with. The Swift bindings only
# export the bridge functions whose features are on, so the app's #if guards
# must match exactly. Both the bindings and the guards derive from this one
# variable — there's no second place to keep in sync. iOS omits the desktop
# import/cd methods macOS pulls in, so its default is oauth-providers,cloudkit,cast.
# `-` (not `:-`) substitutes the default only when the variable is UNSET. The
# baeium (S3-only) build is `BAE_BRIDGE_FEATURES=cast`: casting ships in every
# edition, so `cast` is what anchors a non-empty baeium value here, the way
# `desktop` does on macOS.
BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES-oauth-providers,cloudkit,cast}"
export BAE_BRIDGE_FEATURES

# Map each Rust feature to its Swift compilation condition. A build compiled
# with `oauth-providers` defines BAE_OAUTH_PROVIDERS; one without it must not.
SWIFT_CONDITIONS=""
case ",$BAE_BRIDGE_FEATURES," in
    *,oauth-providers,*) SWIFT_CONDITIONS="$SWIFT_CONDITIONS BAE_OAUTH_PROVIDERS" ;;
esac
case ",$BAE_BRIDGE_FEATURES," in
    *,cloudkit,*) SWIFT_CONDITIONS="$SWIFT_CONDITIONS BAE_CLOUDKIT" ;;
esac
SWIFT_CONDITIONS="$(echo "$SWIFT_CONDITIONS" | xargs)"

# A build without cloudkit signs with the baeium entitlements, which drop the
# iCloud-container keys (a paid account with the iCloud capability is required to
# sign them) but keep keychain access for the cloud keychain — so a self-build
# with free provisioning can ship. The full build leaves CODE_SIGN_ENTITLEMENTS
# at the default Config.xcconfig sets.
ENTITLEMENTS_OVERRIDE=""
case ",$BAE_BRIDGE_FEATURES," in
    *,cloudkit,*) ;;
    *) ENTITLEMENTS_OVERRIDE="CODE_SIGN_ENTITLEMENTS = bae/bae-baeium.entitlements" ;;
esac

if command -v sccache &> /dev/null; then
    export RUSTC_WRAPPER=sccache
fi

rustup target add aarch64-apple-ios aarch64-apple-ios-sim 2>/dev/null || true

# iOS minimum deployment target. Must be >= the prebuilt FFmpeg static libs'
# minos (16.0); a lower target makes the linker reach for runtime symbols
# (`___chkstk_darwin`) that the SDK only vends for the negotiated minimum.
IOS_DEPLOYMENT_TARGET=16.0

# FFmpeg for in-core audio decode. ffmpeg-sys-next locates the libs/headers via
# FFMPEG_DIR (set per-arch on each build below) and emits the -lavcodec/-lavformat
# link flags from it. In the FFMPEG_DIR path it does NOT forward --target/-isysroot
# to bindgen, so without these per-arch clang args bindgen emits HOST-ABI structs
# that compile but corrupt every FFmpeg struct layout at runtime. The libs are
# STATIC .a's built by scripts/build-ffmpeg-ios.sh.
FFMPEG_DEVICE="$(pwd)/bae-ffmpeg/ios/aarch64-apple-ios"
FFMPEG_SIM="$(pwd)/bae-ffmpeg/ios/aarch64-apple-ios-sim"
if [ ! -f "$FFMPEG_DEVICE/lib/libavcodec.a" ]; then
    echo "FFmpeg for iOS not built. Run: ./scripts/build-ffmpeg-ios.sh" >&2
    exit 1
fi

DEVICE_SDK="$(xcrun --sdk iphoneos --show-sdk-path)"
SIM_SDK="$(xcrun --sdk iphonesimulator --show-sdk-path)"
HOST_SDK="$(xcrun --sdk macosx --show-sdk-path)"

# FFmpeg pulls zlib (`uncompress`); the SDK vends it as -lz. The cdylib link step
# (crate-type includes cdylib) needs it spelled out — the staticlib we ship
# defers it to the consuming app, but the cargo build links the cdylib too.
IOS_RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-lz"

run_host_cargo() {
    env -u IPHONEOS_DEPLOYMENT_TARGET \
        -u RUSTFLAGS \
        -u CARGO_ENCODED_RUSTFLAGS \
        -u BINDGEN_EXTRA_CLANG_ARGS \
        SDKROOT="$HOST_SDK" \
        MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}" \
        cargo "$@"
}

echo "Building for iOS device (arm64, $CARGO_PROFILE, features: $BAE_BRIDGE_FEATURES)..."
FFMPEG_DIR="$FFMPEG_DEVICE" \
IPHONEOS_DEPLOYMENT_TARGET="$IOS_DEPLOYMENT_TARGET" \
RUSTFLAGS="$IOS_RUSTFLAGS" \
BINDGEN_EXTRA_CLANG_ARGS="--target=arm64-apple-ios16.0 -isysroot $DEVICE_SDK -I$FFMPEG_DEVICE/include" \
cargo build $CARGO_FLAGS --target aarch64-apple-ios -p bae-bridge --features "$BAE_BRIDGE_FEATURES"

echo "Building for iOS simulator (arm64, $CARGO_PROFILE, features: $BAE_BRIDGE_FEATURES)..."
FFMPEG_DIR="$FFMPEG_SIM" \
IPHONEOS_DEPLOYMENT_TARGET="$IOS_DEPLOYMENT_TARGET" \
RUSTFLAGS="$IOS_RUSTFLAGS" \
SDKROOT="$SIM_SDK" \
BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$SIM_SDK -target arm64-apple-ios16.0-simulator -I$FFMPEG_SIM/include" \
cargo build $CARGO_FLAGS --target aarch64-apple-ios-sim -p bae-bridge --features "$BAE_BRIDGE_FEATURES"

# Write the Swift compilation conditions derived from the feature set. The
# Xcode project includes this file (via configFiles in project.yml) so the #if
# guards in the app track whatever the bridge was built with. DEBUG and the rest
# are preserved via $(inherited).
FEATURES_XCCONFIG="bae-ios/bae/Features.xcconfig"
echo "Writing Swift compilation conditions: ${SWIFT_CONDITIONS:-(none)}"
{
    echo "// Generated by bae-bridge/build-ios.sh from BAE_BRIDGE_FEATURES."
    echo "// Do not edit: it is overwritten on every bridge build and tracks the"
    echo "// cargo feature set the bridge was compiled with."
    echo "SWIFT_ACTIVE_COMPILATION_CONDITIONS = \$(inherited) $SWIFT_CONDITIONS"
    [ -n "$ENTITLEMENTS_OVERRIDE" ] && echo "$ENTITLEMENTS_OVERRIDE"
} > "$FEATURES_XCCONFIG"

echo "Generating Swift bindings..."
SWIFT_BINDINGS_DIR="bae-bridge/swift-bindings-ios"
mkdir -p "$SWIFT_BINDINGS_DIR"
run_host_cargo run --bin uniffi-bindgen generate \
    --library "$CARGO_TARGET_DIR/aarch64-apple-ios/$CARGO_PROFILE/libbae_bridge.a" \
    --language swift \
    --out-dir "$SWIFT_BINDINGS_DIR/"

echo "Generating localization String Catalog (Apple)..."
run_host_cargo run -q -p bae-loc --bin loc-gen -- emit --target apple --out-dir bae-ios/bae/bae

# Install the iOS-flavored bindings into the BaeBridge target (os(iOS)-gated).
# The iOS app must compile against the bindings whose checksum symbols the iOS
# xcframework exports, so build-ios.sh generates into its own scratch directory
# (swift-bindings-ios) and the install step wraps it in its own os-gated file.
./bae-bridge/install-swift-bindings.sh ios

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
rm -rf BaeKit/Frameworks/BaeBridgeFFI-ios.xcframework

mkdir -p "$SWIFT_BINDINGS_DIR/headers"
cp "$SWIFT_BINDINGS_DIR/bae_bridgeFFI.h" "$SWIFT_BINDINGS_DIR/headers/"
cp "$SWIFT_BINDINGS_DIR/bae_bridgeFFI.modulemap" "$SWIFT_BINDINGS_DIR/headers/module.modulemap"

xcodebuild -create-xcframework \
    -library "$DEVICE_MERGED" \
    -headers "$SWIFT_BINDINGS_DIR/headers" \
    -library "$SIM_MERGED" \
    -headers "$SWIFT_BINDINGS_DIR/headers" \
    -output BaeKit/Frameworks/BaeBridgeFFI-ios.xcframework

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  BaeKit/Frameworks/BaeBridgeFFI-ios.xcframework/"
echo "  BaeKit/Sources/BaeBridge/bae_bridge_ios.swift"
