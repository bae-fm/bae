#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

./bae-bridge/build-macos.sh
./bae-bridge/install-swift-bindings.sh macos
cd bae-macos/bae && xcodegen
open bae.xcodeproj
