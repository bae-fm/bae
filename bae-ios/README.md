# bae-ios

iOS app for bae.

## Setting up the Xcode project

Install [xcodegen](https://github.com/yonaskolb/XcodeGen):

```sh
brew install xcodegen
```

Generate the Xcode project:

```sh
cd bae
xcodegen generate
open bae.xcodeproj
```

The app links `bae-bridge/BaeBridgeFFI-ios.xcframework`. Build it with
`bae-bridge/build-ios.sh` from the repo root.
