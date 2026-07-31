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

/// What the remote fetch closure answers with, swapped mid-test to stand in for
/// core re-reading a URL whose bytes changed.
private actor RemoteSource {
    private var image: RemoteImageBytes

    init(image: RemoteImageBytes) {
        self.image = image
    }

    func serve(_ image: RemoteImageBytes) {
        self.image = image
    }

    func fetch() -> RemoteImageBytes {
        image
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

@Suite("ImageStore token staleness")
struct ImageStoreStalenessTests {
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

    @Test("a modified local file misses the decode of its previous contents")
    func localFileMtimeBumpMisses() async throws {
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

        // The user replaced the candidate file. Filesystem timestamps are
        // coarse, so the new date is set explicitly rather than by writing fast.
        try makePngBytes(width: 16, height: 16).write(to: file)
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(60)],
            ofItemAtPath: file.path
        )

        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) == nil,
            "the decode was pinned to the contents that produced it"
        )
    }

    @Test("a changed remote validator replaces the decodes at every size")
    func remoteValidatorChangeReplacesDecodes() async throws {
        let first = try makePngBytes(width: 8, height: 8)
        let second = try makePngBytes(width: 16, height: 16)
        let remote = RemoteSource(
            image: RemoteImageBytes(bytes: first, validator: "v1")
        )
        let store = ImageStore(fetchRemoteImage: { _ in await remote.fetch() })
        let content = ImageContent.remote(url: "https://art.example/cover.jpg")

        _ = try await store.image(content, pointSize: 56, displayScale: 2)
        _ = try await store.image(content, pointSize: 120, displayScale: 2)
        #expect(
            store.cachedImage(content, pointSize: 120, displayScale: 2) != nil
        )

        // Core re-read the URL and the bytes came back different. A size the
        // store already holds is served from the cache and asks core nothing, so
        // the fetch that learns this is a size it hasn't decoded yet.
        await remote.serve(
            RemoteImageBytes(bytes: second, validator: "v2")
        )
        let reloaded = try await store.image(
            content,
            pointSize: 200,
            displayScale: 2
        )

        #expect(reloaded != nil)
        #expect(
            store.cachedImage(content, pointSize: 200, displayScale: 2) != nil,
            "the decode of the new bytes stays"
        )
        for stalePointSize in [56.0, 120.0] {
            #expect(
                store.cachedImage(
                    content,
                    pointSize: stalePointSize,
                    displayScale: 2
                ) == nil,
                "decodes of the old bytes are stale at every size, not just the one reloaded"
            )
        }
    }

    @Test("an unchanged remote validator keeps the decodes it already made")
    func remoteValidatorUnchangedKeepsDecodes() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let remote = RemoteSource(
            image: RemoteImageBytes(bytes: bytes, validator: "v1")
        )
        let store = ImageStore(fetchRemoteImage: { _ in await remote.fetch() })
        let content = ImageContent.remote(url: "https://art.example/cover.jpg")

        _ = try await store.image(content, pointSize: 56, displayScale: 2)
        _ = try await store.image(content, pointSize: 120, displayScale: 2)
        // A third size fetches again and gets the same validator back.
        _ = try await store.image(content, pointSize: 200, displayScale: 2)

        #expect(
            store.cachedImage(content, pointSize: 120, displayScale: 2) != nil
        )
        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) != nil
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

        cache.removeImage(for: "cover-a")
        #expect(cache.image(for: "cover-a") == nil)
        #expect(cache.image(for: "cover-b") === secondImage)
    }
}
