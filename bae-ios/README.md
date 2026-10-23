# bae-ios

The iOS skeleton: start-up and release machinery, not a UI. Launch starts
telemetry (`BaeDiagnostics.configure`), crash reporting
(`BaeCrashReporting.configure`), bae's directory, the host and the keychain,
and one screen shows the sheep, the wordmark, and either the step that stopped
the launch or what the host's library listing returned. The phone UI is
rebuilt from the Android app later; until then this app has no views, stores
or strings to update when a bridge shape changes. It only has to keep
compiling against the regenerated bindings and pass the iOS CI job.

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

`bae-ios/run.sh` builds and runs the app on a simulator; `bae-ios/open.sh`
builds the bridge and opens the project.

## Shared package

The generated bridge bindings and the launch pieces the app uses live in the
`BaeKit` Swift package at the repo root, shared with the macOS app. The app
depends on it as a local package (`packages.BaeKit: ../../BaeKit` in
`bae/project.yml`), so a single `import BaeKit` brings both the shared layer
and the re-exported bridge types. `build-ios.sh` installs the iOS uniffi
bindings into `BaeKit/Sources/BaeBridge` and writes the platform-conditional
`BaeKit/Frameworks/BaeBridgeFFI-ios.xcframework` binary target.
