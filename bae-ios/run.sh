#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    echo "Usage: $0 [--skip-rust]"
    echo "  Builds and runs the iOS app on simulator. Pass --skip-rust to skip the Rust build."
    exit 0
fi

cd "$(dirname "$0")/.."

if [[ "${1:-}" != "--skip-rust" ]]; then
    ./bae-bridge/build-ios.sh
fi

cd bae-ios/bae && xcodegen && cd ../..
xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPhone 16' -derivedDataPath build/ios build

xcrun simctl boot "iPhone 16" 2>/dev/null || true
open -a Simulator
xcrun simctl install booted build/ios/Build/Products/Debug-iphonesimulator/bae.app
xcrun simctl launch booted fm.bae.bae
