#!/usr/bin/env bash
set -euo pipefail

SKIP_RUST=false
RELEASE=false
OPEN=true

for arg in "$@"; do
    case "$arg" in
        --skip-rust) SKIP_RUST=true ;;
        --release) RELEASE=true ;;
        --no-open) OPEN=false ;;
        -h|--help)
            echo "Usage: $0 [--skip-rust] [--release] [--no-open]"
            echo "  Builds (and optionally runs) the macOS app."
            echo "  --skip-rust  Skip the Rust bridge build"
            echo "  --release    Build Rust in release mode, Swift in Release config"
            echo "  --no-open     Build only, don't launch the app"
            exit 0
            ;;
        *) echo "Unknown flag: $arg"; exit 1 ;;
    esac
done

cd "$(dirname "$0")/.."

if [[ "$SKIP_RUST" == false ]]; then
    if [[ "$RELEASE" == true ]]; then
        ./bae-bridge/build-macos.sh --release
    else
        ./bae-bridge/build-macos.sh
    fi
    cp bae-bridge/swift-bindings/bae_bridge.swift bae-macos/bae/bae/bae_bridge.swift
fi

if [[ "$RELEASE" == true ]]; then
    CONFIG=Release
else
    CONFIG=Debug
fi

cd bae-macos/bae && xcodegen && cd ../..
xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration "$CONFIG" -derivedDataPath build build

if [[ "$OPEN" == true ]]; then
    open "build/Build/Products/$CONFIG/bae.app" --env BAE_IMPORT_TRACE=1
fi
