#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

./bae-bridge/build-macos.sh
cp bae-bridge/swift-bindings/bae_bridge.swift bae-macos/bae/bae/bae_bridge.swift
cd bae-macos/bae && xcodegen
open bae.xcodeproj
