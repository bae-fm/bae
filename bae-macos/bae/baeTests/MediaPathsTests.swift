import AppKit
import Testing

@testable import BaeKit
@testable import bae

/// PNG bytes of a solid-color image, for feeding `MediaPaths` fetch closures.
private func makePngBytes(width: Int, height: Int) throws -> Data {
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let context = try #require(
        CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
    )
    context.setFillColor(CGColor(gray: 0.5, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let cgImage = try #require(context.makeImage())
    let rep = NSBitmapImageRep(cgImage: cgImage)
    return try #require(rep.representation(using: .png, properties: [:]))
}

@Suite("MediaPaths")
struct MediaPathsTests {
    @Test(
        "cachedLibraryImage is nil before a load and serves the decoded instance after"
    )
    func cachedLibraryImageRoundTrip() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let paths = MediaPaths(fetchLibraryImageBytes: { _ in bytes })
        let source = LibraryImageSource.image(
            BridgeImageRef(id: "rel-1", version: "1", imageType: .cover)
        )

        #expect(
            paths.cachedLibraryImage(source, pointSize: 56, displayScale: 2)
                == nil
        )

        let loaded = try #require(
            try await paths.libraryImage(
                source,
                pointSize: 56,
                displayScale: 2
            )
        )

        #expect(
            paths.cachedLibraryImage(source, pointSize: 56, displayScale: 2)
                === loaded
        )
    }

    @Test("cachedLibraryImage misses on a different pixel size")
    func cachedLibraryImageKeysOnPixelSize() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let paths = MediaPaths(fetchLibraryImageBytes: { _ in bytes })
        let source = LibraryImageSource.image(
            BridgeImageRef(id: "rel-1", version: "1", imageType: .cover)
        )

        _ = try await paths.libraryImage(
            source,
            pointSize: 56,
            displayScale: 2
        )

        #expect(
            paths.cachedLibraryImage(source, pointSize: 400, displayScale: 2)
                == nil
        )
    }
}
