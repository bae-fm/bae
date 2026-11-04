// swift-tools-version: 6.0
import PackageDescription

// The bridge's one xcframework holds a slice for each platform whose bridge
// this checkout built: bae-bridge/build-macos.sh and build-ios.sh each write
// their own platform's slices into it and keep the others
// (bae-bridge/xcframework.py). The manifest names it without asking what is
// on disk: SwiftPM caches a manifest's evaluation by its text, so a manifest
// that checked for files kept the answer it got first.

let package = Package(
    name: "BaeKit",
    defaultLocalization: "en",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [.library(name: "BaeKit", targets: ["BaeKit", "BaeBridge"])],
    dependencies: [
        .package(
            url: "https://github.com/getsentry/sentry-cocoa",
            exact: "9.18.0"
        )
    ],
    targets: [
        .binaryTarget(
            name: "BaeBridgeFFI",
            path: "Frameworks/BaeBridgeFFI.xcframework"
        ),
        .target(name: "BaeBridge", dependencies: ["BaeBridgeFFI"]),
        .target(
            name: "BaeKit",
            dependencies: [
                "BaeBridge",
                .product(name: "Sentry", package: "sentry-cocoa"),
            ],
            resources: [.process("Resources")]
        ),
    ]
)
