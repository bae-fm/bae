# bae-macos

Native macOS app for bae, built with SwiftUI.

## Requirements

- macOS 14.0+
- Xcode 26+
- Rust toolchain with `aarch64-apple-darwin` target
- FFmpeg (`brew install ffmpeg`)
- xcodegen (`brew install xcodegen`)

Install the Rust target if you haven't:

    rustup target add aarch64-apple-darwin

## Build

All commands run from the repo root.

1. Build the Rust FFI bridge:

       cd bae-bridge
       ./build-macos.sh
       cd ..

2. Copy outputs into the Xcode project:

       cp -r bae-bridge/BaeBridgeFFI.xcframework bae-macos/
       cp bae-bridge/swift-bindings/bae_bridge.swift bae-macos/bae/bae/

3. Generate the Xcode project:

       cd bae-macos/bae
       xcodegen
       cd ../..

4. Open in Xcode:

       open bae-macos/bae/bae.xcodeproj

## Running

**Debug** (default): `Cmd+R` in Xcode. Swift code is unoptimized with debug symbols.

**Release**: Product > Scheme > Edit Scheme > Run > Build Configuration > Release, then `Cmd+R`. Or from the command line:

    xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Release build

The Rust side is always built with `--release` by `build-macos.sh`. The Debug/Release toggle in Xcode only affects Swift compilation.

## Formatting

Swift code is formatted with [swift-format](https://github.com/swiftlang/swift-format), which ships with Xcode (run via `xcrun swift-format`). The pre-commit hook formats `bae-macos/bae/bae/` automatically. Config is in `.swift-format`.

## Data

The app discovers libraries from `~/.bae/libraries/`. Run the app first to create or import a library if you don't have one.
