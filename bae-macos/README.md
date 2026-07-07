# bae-macos

Native macOS app for bae, built with SwiftUI.

## Requirements

- macOS 14.0+
- Xcode 26+
- Rust toolchain with `aarch64-apple-darwin` target
- FFmpeg from the bae-ffmpeg fork (`./scripts/setup-ffmpeg.sh` from the repo
  root) -- bae links these prebuilt libs, not a system/Homebrew ffmpeg
- xcodegen (`brew install xcodegen`)

Install the Rust target if you haven't:

    rustup target add aarch64-apple-darwin

## Shared package

The data/service/domain layer and the generated bridge bindings live in the
`BaeKit` Swift package at the repo root, shared with the iOS app. The app
depends on it as a local package (`packages.BaeKit: ../../BaeKit` in
`bae/project.yml`), so a
single `import BaeKit` brings both the shared layer and the re-exported bridge
types. `build-macos.sh` produces the package's inputs: it installs the macOS
uniffi bindings into `BaeKit/Sources/BaeBridge` and writes the
`BaeKit/Frameworks/BaeBridgeFFI.xcframework` binary target.

## Build

All commands run from the repo root.

1. Build the Rust FFI bridge and install its outputs into BaeKit:

       ./bae-bridge/build-macos.sh

2. Generate the Xcode project:

       cd bae-macos/bae
       xcodegen
       cd ../..

3. Open in Xcode:

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
