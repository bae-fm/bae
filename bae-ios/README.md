# bae-ios

iOS app for bae.

## Setting up the Xcode project

Install [xcodegen](https://github.com/yonaskolb/XcodeGen):

```sh
brew install xcodegen
```

Build the Rust bridge (installs the iOS bindings and the xcframework into the
shared BaeKit package), then generate the Xcode project:

```sh
./bae-bridge/build-ios.sh
cd bae-ios/bae
xcodegen generate
open bae.xcodeproj
```

## Shared package

The data/service/domain layer and the generated bridge bindings live in the
`BaeKit` Swift package at the repo root, shared with the macOS app.
The app depends on it as a local package (`packages.BaeKit: ../../BaeKit` in
`bae/project.yml`), so a single `import BaeKit` brings both the shared layer and the re-exported
bridge types. `build-ios.sh` installs the iOS uniffi bindings into
`BaeKit/Sources/BaeBridge` and writes the platform-conditional
`BaeKit/Frameworks/BaeBridgeFFI-ios.xcframework` binary target.
