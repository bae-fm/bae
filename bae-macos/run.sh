#!/usr/bin/env bash
set -euo pipefail

SKIP_RUST=false
RELEASE=false
OPEN=true
EDITION=bae

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-rust) SKIP_RUST=true ;;
        --release) RELEASE=true ;;
        --no-open) OPEN=false ;;
        --edition)
            if [[ $# -lt 2 ]]; then
                echo "Missing value for --edition"
                exit 1
            fi
            EDITION="${2:-}"
            shift
            ;;
        --edition=*)
            EDITION="${1#*=}"
            ;;
        -h|--help)
            echo "Usage: $0 [--skip-rust] [--release] [--no-open] [--edition bae|baeium]"
            echo "  Builds (and optionally runs) the macOS app."
            echo "  --skip-rust  Skip the Rust bridge build"
            echo "  --release    Build Rust in release mode, Swift in Release config"
            echo "  --no-open    Build only, don't launch the app"
            echo "  --edition    Build bae or baeium"
            exit 0
            ;;
        *) echo "Unknown flag: $1"; exit 1 ;;
    esac
    shift
done

cd "$(dirname "$0")/.."

case "$EDITION" in
    bae)
        BAE_BRIDGE_FEATURES_VALUE="oauth-providers,cloudkit,desktop"
        BUNDLE_ID="fm.bae.desktop"
        PRODUCT_NAME="bae"
        ;;
    baeium)
        BAE_BRIDGE_FEATURES_VALUE="desktop"
        BUNDLE_ID="fm.bae.desktop.baeium"
        PRODUCT_NAME="baeium"
        ;;
    *)
        echo "Unknown edition: $EDITION"
        exit 1
        ;;
esac

if [[ "$SKIP_RUST" == false ]]; then
    if [[ "$RELEASE" == true ]]; then
        BAE_BRIDGE_FEATURES="$BAE_BRIDGE_FEATURES_VALUE" ./bae-bridge/build-macos.sh --release
    else
        BAE_BRIDGE_FEATURES="$BAE_BRIDGE_FEATURES_VALUE" ./bae-bridge/build-macos.sh
    fi
    cp bae-bridge/swift-bindings-macos/bae_bridge.swift bae-macos/bae/bae/bae_bridge.swift
fi

if [[ "$RELEASE" == true ]]; then
    CONFIG=Release
else
    CONFIG=Debug
fi

cd bae-macos/bae && xcodegen && cd ../..
xcodebuild -project bae-macos/bae/bae.xcodeproj \
    -scheme bae \
    -configuration "$CONFIG" \
    -derivedDataPath build \
    PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
    PRODUCT_NAME="$PRODUCT_NAME" \
    build

if [[ "$OPEN" == true ]]; then
    open "build/Build/Products/$CONFIG/$PRODUCT_NAME.app" --env BAE_IMPORT_TRACE=1
fi
