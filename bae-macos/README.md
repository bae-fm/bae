# bae-macos

Native macOS app for bae: SwiftUI views in an AppKit app shell. An
`NSApplicationDelegate` owns startup, the main menu is an `NSMenu` built at
launch, and each window is an `NSWindow` hosting a SwiftUI root view.

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
`BaeKit` Swift package at the repo root; the iOS skeleton uses its launch
pieces. The app depends on it as a local package (`packages.BaeKit:
../../BaeKit` in `bae/project.yml`), so a single `import BaeKit` brings both
the shared layer and the re-exported bridge types. `build-macos.sh` produces the package's inputs: it installs the macOS
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

The Xcode build runs `build-macos.sh` for the same configuration: Debug builds the Rust bridge in debug with the `test-keyring` and `dev-reset` features, Release builds it with `--release` and neither.

## Formatting

Swift code is formatted with [swift-format](https://github.com/swiftlang/swift-format), which ships with Xcode (run via `xcrun swift-format`). The pre-commit hook formats `bae-macos/bae/bae/` automatically. Config is in `.swift-format`.

## Data

The app discovers libraries from `~/.bae/libraries/`. Run the app first to create or import a library if you don't have one.

## Resetting the dev library

`scripts/reset-bae.py` wipes this Mac's bae state and recreates one library from
`~/.config/bae-dev/profile.toml` (mode 600, never committed; the script's
docstring lists its fields). It quits the running app, shows the plan, and on
confirmation launches the Debug app from `bae-macos/run.sh` with
`--dev-reset <profile>`. Before any library opens, the app removes every library
the way Remove Library does, deletes their saved restore codes, empties the
profile's S3 key prefix (deleting every object under it and aborting its
unfinished uploads, whatever wrote them, through coven's `empty_s3_prefix`),
deletes `~/.bae`, then creates the profile's library and sets its Discogs
token, metadata preferences and S3 storage. Any failure stops the reset and
shows on the welcome screen; Console logs each step under the `DevReset`
category, including how many objects and uploads the emptying removed. A
versioned bucket keeps the deleted objects' earlier versions. Release builds
don't contain `--dev-reset`.
