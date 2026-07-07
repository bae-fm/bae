import AppKit
import Testing

@testable import bae

private func makeDecodedImage(width: Int, height: Int) throws -> NSImage {
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
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let cgImage = try #require(context.makeImage())
    return NSImage(
        cgImage: cgImage,
        size: NSSize(width: width, height: height)
    )
}

@Suite("LibraryImageCache")
struct LibraryImageCacheTests {
    @Test("stores images by key and computes decoded byte cost")
    func storesImagesByKeyAndComputesDecodedByteCost() throws {
        let cache = LibraryImageCache(totalCostLimit: 1024)
        let firstImage = try makeDecodedImage(width: 2, height: 3)
        let secondImage = try makeDecodedImage(width: 4, height: 5)

        #expect(LibraryImageCache.decodedByteCost(of: firstImage) == 24)
        #expect(LibraryImageCache.decodedByteCost(of: secondImage) == 80)

        cache.store(firstImage, for: "cover-a")
        cache.store(secondImage, for: "cover-b")

        #expect(cache.image(for: "cover-a") === firstImage)
        #expect(cache.image(for: "cover-b") === secondImage)
    }
}
