#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-macos}"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for macOS (arm64). Debug by default."
    exit 0
fi

CARGO_PROFILE="debug"
CARGO_FLAGS=""
if [[ "${1:-}" == "--release" ]]; then
    CARGO_PROFILE="release"
    CARGO_FLAGS="--release"
fi

# Use sccache if available
if command -v sccache &> /dev/null; then
    export RUSTC_WRAPPER=sccache
fi

echo "Building for macOS (arm64, $CARGO_PROFILE)..."
cargo build $CARGO_FLAGS --target aarch64-apple-darwin -p bae-bridge --features cloudkit,desktop

echo "Generating Swift bindings..."
mkdir -p bae-bridge/swift-bindings
cargo run --bin uniffi-bindgen generate \
    --library "$CARGO_TARGET_DIR/aarch64-apple-darwin/$CARGO_PROFILE/libbae_bridge.a" \
    --language swift \
    --out-dir bae-bridge/swift-bindings/

echo "Creating XCFramework..."
rm -rf bae-macos/BaeBridgeFFI.xcframework

mkdir -p bae-bridge/swift-bindings/headers
cp bae-bridge/swift-bindings/bae_bridgeFFI.h bae-bridge/swift-bindings/headers/
cp bae-bridge/swift-bindings/bae_bridgeFFI.modulemap bae-bridge/swift-bindings/headers/module.modulemap

xcodebuild -create-xcframework \
    -library "$CARGO_TARGET_DIR/aarch64-apple-darwin/$CARGO_PROFILE/libbae_bridge.a" \
    -headers bae-bridge/swift-bindings/headers \
    -output bae-macos/BaeBridgeFFI.xcframework

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  bae-macos/BaeBridgeFFI.xcframework/"
echo "  bae-bridge/swift-bindings/bae_bridge.swift"
