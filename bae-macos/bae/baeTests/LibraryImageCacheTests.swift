import AppKit
import Testing

@testable import bae

private func makeDecodedImage() throws -> NSImage {
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let context = try #require(
        CGContext(
            data: nil,
            width: 2,
            height: 3,
            bitsPerComponent: 8,
            bytesPerRow: 8,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
    )
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: 2, height: 3))
    let cgImage = try #require(context.makeImage())
    return NSImage(
        cgImage: cgImage,
        size: NSSize(width: 2, height: 3)
    )
}

@Suite("LibraryImageCache")
struct LibraryImageCacheTests {
    @Test("evicts entries by decoded byte cost")
    func evictsEntriesByDecodedByteCost() throws {
        let cache = LibraryImageCache(totalCostLimit: 24)
        let firstImage = try makeDecodedImage()
        let secondImage = try makeDecodedImage()

        #expect(LibraryImageCache.decodedByteCost(of: firstImage) == 24)

        cache.store(firstImage, for: "cover-a")
        #expect(cache.image(for: "cover-a") === firstImage)

        cache.store(secondImage, for: "cover-b")
        #expect(cache.image(for: "cover-a") == nil)
        #expect(cache.image(for: "cover-b") === secondImage)
    }
}
