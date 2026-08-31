import AppKit
import Foundation
import Testing

@testable import BaeKit
@testable import bae

/// PNG bytes of a solid-color image, for feeding `ImageStore` fetch closures.
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
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let cgImage = try #require(context.makeImage())
    let rep = NSBitmapImageRep(cgImage: cgImage)
    return try #require(rep.representation(using: .png, properties: [:]))
}

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

private func coverRef(version: String) -> BridgeImageRef {
    BridgeImageRef(id: "rel-1", version: version, imageType: .cover)
}

private actor FetchCount {
    private var value = 0

    func increment() {
        value += 1
    }

    func read() -> Int {
        value
    }
}

@Suite("ImageStore cache")
struct ImageStoreCacheTests {
    @Test(
        "cachedImage is nil before a load and serves the decoded instance after"
    )
    func cachedImageRoundTrip() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore(fetchLibraryImageBytes: { _ in bytes })
        let content = ImageContent.libraryImage(coverRef(version: "1"))

        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) == nil
        )

        let loaded = try #require(
            try await store.image(content, pointSize: 56, displayScale: 2)
        )

        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2)
                === loaded
        )
    }

    @Test("cachedImage misses on a different pixel size")
    func cachedImageKeysOnPixelSize() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore(fetchLibraryImageBytes: { _ in bytes })
        let content = ImageContent.libraryImage(coverRef(version: "1"))

        _ = try await store.image(content, pointSize: 56, displayScale: 2)

        #expect(
            store.cachedImage(content, pointSize: 400, displayScale: 2) == nil
        )
    }

    @Test("raw bytes decode but are never cached")
    func rawBytesAreNeverCached() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore()
        let content = ImageContent.bytes(bytes)

        let loaded = try await store.image(
            content,
            pointSize: 56,
            displayScale: 2
        )
        #expect(loaded != nil)
        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) == nil,
            "the caller holds these bytes' only identity — nothing to cache under"
        )
    }

    @Test("a library image with no bytes resolves to nothing")
    func absentLibraryImageResolvesToNil() async throws {
        let store = ImageStore(fetchLibraryImageBytes: { _ in nil })
        let content = ImageContent.libraryImage(coverRef(version: "1"))

        #expect(
            try await store.image(content, pointSize: 56, displayScale: 2)
                == nil
        )
    }

    @Test("concurrent requests share one fetch and decode")
    func concurrentRequestsShareLoad() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let fetchCount = FetchCount()
        let store = ImageStore(fetchLibraryImageBytes: { _ in
            await fetchCount.increment()
            try await Task.sleep(for: .milliseconds(100))
            return bytes
        })
        let content = ImageContent.libraryImage(coverRef(version: "1"))

        async let first = store.image(
            content,
            pointSize: 56,
            displayScale: 2
        )
        async let second = store.image(
            content,
            pointSize: 56,
            displayScale: 2
        )

        let images = try await [first, second]
        #expect(images[0] === images[1])
        #expect(await fetchCount.read() == 1)
    }
}

@Suite("ImageStore bucket isolation")
struct ImageStoreBucketTests {
    @Test("filling the release-image bucket never evicts a library image")
    func releaseImagePressureLeavesLibraryImagesAlone() async throws {
        let bytes = try makePngBytes(width: 64, height: 64)
        // The release bucket holds roughly one decode; the library bucket has
        // room to spare. Pressure on one must not reach the other.
        let store = ImageStore(
            fetchLibraryImageBytes: { _ in bytes },
            fetchReleaseImageBytes: { _, _ in bytes },
            budgets: ImageStoreBudgets(
                libraryImage: 4 * 1024 * 1024,
                releaseImage: 64 * 64 * 4,
                remote: 1024,
                localFile: 1024
            )
        )
        let libraryContent = ImageContent.libraryImage(coverRef(version: "1"))
        _ = try await store.image(
            libraryContent,
            pointSize: 64,
            displayScale: 1
        )

        for index in 0..<32 {
            _ = try await store.image(
                .releaseImage(
                    releaseId: "rel-1",
                    source: .releaseFile(fileId: "file-\(index)")
                ),
                pointSize: 64,
                displayScale: 1
            )
        }

        #expect(
            store.cachedImage(
                libraryContent,
                pointSize: 64,
                displayScale: 1
            ) != nil
        )
    }
}

@Suite("ImageStore content identity")
struct ImageStoreContentIdentityTests {
    @Test("a new content version misses the previous version's decode")
    func versionBumpMisses() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore(fetchLibraryImageBytes: { _ in bytes })

        _ = try await store.image(
            .libraryImage(coverRef(version: "1")),
            pointSize: 56,
            displayScale: 2
        )

        #expect(
            store.cachedImage(
                .libraryImage(coverRef(version: "2")),
                pointSize: 56,
                displayScale: 2
            ) == nil
        )
    }

    @Test("a cover slot and the same cover read directly share a version key")
    func coverSlotKeysOnItsVersion() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore(fetchReleaseImageBytes: { _, _ in bytes })
        let slot = ImageContent.releaseImage(
            releaseId: "rel-1",
            source: .cover(image: coverRef(version: "1"))
        )

        _ = try await store.image(slot, pointSize: 56, displayScale: 2)
        #expect(store.cachedImage(slot, pointSize: 56, displayScale: 2) != nil)

        #expect(
            store.cachedImage(
                .releaseImage(
                    releaseId: "rel-1",
                    source: .cover(image: coverRef(version: "2"))
                ),
                pointSize: 56,
                displayScale: 2
            ) == nil,
            "a replaced cover moves the slot's key"
        )
    }

    @Test("a local cache hit does not read the source file again")
    func localCacheHitDoesNotReadSource() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("candidate.png")
        try makePngBytes(width: 8, height: 8).write(to: file)

        let store = ImageStore()
        let content = ImageContent.localFile(path: file.path)
        _ = try await store.image(content, pointSize: 56, displayScale: 2)
        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) != nil
        )

        try FileManager.default.removeItem(at: file)

        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) != nil,
            "the content address remains the cache identity"
        )
    }
}

@Suite("DecodedImageCache")
struct DecodedImageCacheTests {
    @Test("stores images by key and computes decoded byte cost")
    func storesImagesByKeyAndComputesDecodedByteCost() throws {
        let cache = DecodedImageCache(totalCostLimit: 1024)
        let firstImage = try makeDecodedImage(width: 2, height: 3)
        let secondImage = try makeDecodedImage(width: 4, height: 5)

        #expect(DecodedImageCache.decodedByteCost(of: firstImage) == 24)
        #expect(DecodedImageCache.decodedByteCost(of: secondImage) == 80)

        cache.store(firstImage, for: "cover-a")
        cache.store(secondImage, for: "cover-b")

        #expect(cache.image(for: "cover-a") === firstImage)
        #expect(cache.image(for: "cover-b") === secondImage)
    }
}
