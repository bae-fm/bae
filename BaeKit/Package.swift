import Foundation
// swift-tools-version: 6.0
import PackageDescription

let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

// Declare a binary target only when its xcframework actually exists on disk.
// SwiftPM validates every declared binaryTarget's artifact at resolve time,
// regardless of the platform the dependency is conditioned on — so declaring the
// iOS xcframework unconditionally breaks a macOS-only build (which never runs
// build-ios.sh, so the iOS artifact is absent), and vice versa. Check the
// xcframework's Info.plist (not just the directory) so a leftover empty dir
// doesn't read as a real artifact. If neither exists, BaeBridge has no binary
// dependency and resolves fine; compiling the bindings then fails with the same
// missing-module error as a missing-bindings build — the documented failure mode.
func xcframeworkExists(_ name: String) -> Bool {
    let plist =
        packageDir
        .appendingPathComponent("Frameworks")
        .appendingPathComponent(name)
        .appendingPathComponent("Info.plist")
    return FileManager.default.fileExists(atPath: plist.path)
}

var binaryTargets: [Target] = []
var bridgeDependencies: [Target.Dependency] = []
if xcframeworkExists("BaeBridgeFFI.xcframework") {
    binaryTargets.append(
        .binaryTarget(
            name: "BaeBridgeFFI",
            path: "Frameworks/BaeBridgeFFI.xcframework"
        )
    )
    bridgeDependencies.append(
        .target(name: "BaeBridgeFFI", condition: .when(platforms: [.macOS]))
    )
}
if xcframeworkExists("BaeBridgeFFI-ios.xcframework") {
    binaryTargets.append(
        .binaryTarget(
            name: "BaeBridgeFFIiOS",
            path: "Frameworks/BaeBridgeFFI-ios.xcframework"
        )
    )
    bridgeDependencies.append(
        .target(name: "BaeBridgeFFIiOS", condition: .when(platforms: [.iOS]))
    )
}

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
    targets: binaryTargets + [
        .target(name: "BaeBridge", dependencies: bridgeDependencies),
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
