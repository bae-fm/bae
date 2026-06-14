#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

./bae-bridge/build-ios.sh
cd bae-ios/bae && xcodegen
open bae.xcodeproj
