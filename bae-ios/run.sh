#!/usr/bin/env bash
set -euo pipefail

SKIP_RUST=false
EDITION=bae

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-rust) SKIP_RUST=true ;;
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
            echo "Usage: $0 [--skip-rust] [--edition bae|baeium]"
            echo "  Builds and runs the iOS app on simulator."
            echo "  --skip-rust  Skip the Rust bridge build"
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
        BAE_BRIDGE_FEATURES_VALUE="oauth-providers,cloudkit"
        BUNDLE_ID="fm.bae.bae"
        PRODUCT_NAME="bae"
        ;;
    baeium)
        BAE_BRIDGE_FEATURES_VALUE=""
        BUNDLE_ID="fm.bae.bae.baeium"
        PRODUCT_NAME="baeium"
        ;;
    *)
        echo "Unknown edition: $EDITION"
        exit 1
        ;;
esac

if [[ "$SKIP_RUST" == false ]]; then
    BAE_BRIDGE_FEATURES="$BAE_BRIDGE_FEATURES_VALUE" ./bae-bridge/build-ios.sh
fi

cd bae-ios/bae && xcodegen && cd ../..
xcodebuild -project bae-ios/bae/bae.xcodeproj \
    -scheme bae \
    -configuration Debug \
    -sdk iphonesimulator \
    -destination 'platform=iOS Simulator,name=iPhone 16' \
    -derivedDataPath build/ios \
    PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
    PRODUCT_NAME="$PRODUCT_NAME" \
    build

xcrun simctl boot "iPhone 16" 2>/dev/null || true
open -a Simulator
xcrun simctl install booted "build/ios/Build/Products/Debug-iphonesimulator/$PRODUCT_NAME.app"
xcrun simctl launch booted "$BUNDLE_ID"
