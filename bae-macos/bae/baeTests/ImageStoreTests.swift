import AppKit
import Foundation
import Testing

@testable import BaeKit
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

/// Holds every fetch that reaches it until `open`, so a test decides when a load
/// in flight may finish.
private actor FetchGate {
    private var isOpen = false
    private var held: [CheckedContinuation<Void, Never>] = []

    func pass() async {
        if isOpen {
            return
        }
        await withCheckedContinuation { held.append($0) }
    }

    func open() {
        isOpen = true
        for fetch in held {
            fetch.resume()
        }
        held = []
    }
}

/// A fetch that waits until it is cancelled, announcing its start and its
/// cancellation.
private final class CancellableFetch: Sendable {
    enum Event: Sendable {
        case started
        case cancelled
    }

    let events: AsyncStream<Event>
    private let continuation: AsyncStream<Event>.Continuation

    init() {
        (events, continuation) = AsyncStream.makeStream()
    }

    func wait() async throws {
        continuation.yield(.started)
        do {
            try await Task.sleep(for: .seconds(600))
        }
        catch {
            continuation.yield(.cancelled)
            throw error
        }
    }
}

/// Runs its jobs one at a time, in the order they were enqueued.
private final class SerialTaskExecutor: TaskExecutor {
    private let queue = DispatchQueue(
        label: "ImageStoreTests.SerialTaskExecutor"
    )

    func enqueue(_ job: consuming ExecutorJob) {
        let job = UnownedJob(job)
        queue.async {
            job.runSynchronously(on: self.asUnownedTaskExecutor())
        }
    }
}

private actor PictureAsks {
    private(set) var asks: [String] = []

    func record(_ pixels: UInt32?, _ turn: BridgePictureTurn) {
        let size = pixels.map { "\($0) px" } ?? "original"
        let turnName =
            switch turn {
            case .visible: "visible"
            case .prefetch: "prefetch"
            }
        asks.append("\(size) \(turnName)")
    }
}

private actor PixelReads {
    private(set) var pixels: [UInt32?] = []

    func record(_ value: UInt32?) {
        pixels.append(value)
    }
}

@Suite("ImageStore cache")
struct ImageStoreCacheTests {
    @Test(
        "cachedImage is nil before a load and serves the decoded instance after"
    )
    func cachedImageRoundTrip() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in bytes })
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
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in bytes })
        let content = ImageContent.libraryImage(coverRef(version: "1"))

        _ = try await store.image(content, pointSize: 56, displayScale: 2)

        #expect(
            store.cachedImage(content, pointSize: 400, displayScale: 2) == nil
        )
    }

    @Test(
        "provider art is fetched for the slot's pixel size, and a viewer's decode for the original"
    )
    func remoteArtIsFetchedForTheSlotsPixelSize() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let asked = PixelReads()
        let store = ImageStore(fetchRemoteImage: { _, pixels in
            await asked.record(pixels)
            return bytes
        })
        let content = ImageContent.remote(
            BridgeRemoteImageSet(
                url: "https://images.example/front.jpg",
                downscaled: [
                    BridgeDownscaledCopy(
                        url: "https://images.example/front-150.jpg",
                        maxEdge: 150
                    )
                ]
            )
        )

        _ = try await store.image(content, pointSize: 40, displayScale: 2)
        _ = try await store.image(content, pointSize: 260, displayScale: 2)
        _ = try await store.decodeSource(for: content, at: .nativeResolution)

        #expect(await asked.pixels == [80, 520, nil])
    }

    @Test(
        "a picture of the import list is asked of core at the slot's pixels, a viewer's decode for the original, and a warm as a prefetch"
    )
    func importPicturesAskCoreForTheSlotsPixels() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let asked = PictureAsks()
        let store = ImageStore(fetchPicture: { _, pixels, turn in
            await asked.record(pixels, turn)
            return bytes
        })
        let content = ImageContent.localFile(
            BridgeFileVersion(
                path: Data("/folder/front.png".utf8),
                size: 1_000,
                modifiedAtNs: 1_000
            )
        )

        _ = try await store.image(content, pointSize: 40, displayScale: 2)
        _ = try await store.fullResolutionImage(content, displayScale: 2)
        await store.warm([content], pointSize: 90, displayScale: 2)

        #expect(
            await asked.asks == [
                "80 px visible", "original visible", "180 px prefetch",
            ]
        )
    }

    @Test("a library image with no bytes resolves to nothing")
    func absentLibraryImageResolvesToNil() async throws {
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in nil })
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
        let gate = FetchGate()
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in
            await fetchCount.increment()
            await gate.pass()
            return bytes
        })
        let content = ImageContent.libraryImage(coverRef(version: "1"))

        // Both requests and this body share one serial executor. Each request
        // runs until it waits on a load, and the yield queues this body behind
        // them, so the fetch is let go only once both requests are in flight.
        let images = try await withTaskExecutorPreference(
            SerialTaskExecutor()
        ) {
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
            await Task.yield()
            await gate.open()
            return try await [first, second]
        }
        #expect(images[0] === images[1])
        #expect(await fetchCount.read() == 1)
    }

    /// A tile scrolled away stops wanting its cover. The read it started
    /// waits in core until storage connects, so it is cancelled with its last
    /// request rather than left waiting.
    @Test("cancelling the last request for a load cancels its fetch")
    func lastCancelledRequestCancelsTheFetch() async throws {
        let fetch = CancellableFetch()
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in
            try await fetch.wait()
            return nil
        })
        let content = ImageContent.libraryImage(coverRef(version: "1"))
        let executor = SerialTaskExecutor()

        // Both requests join the one load before either is cancelled.
        let (first, second) = await withTaskExecutorPreference(executor) {
            let first = Task(executorPreference: executor) {
                try await store.image(content, pointSize: 56, displayScale: 2)
            }
            let second = Task(executorPreference: executor) {
                try await store.image(content, pointSize: 56, displayScale: 2)
            }
            await Task.yield()
            return (first, second)
        }
        var events = fetch.events.makeAsyncIterator()
        #expect(await events.next() == .started)

        first.cancel()
        second.cancel()
        #expect(await events.next() == .cancelled)
        await #expect(throws: CancellationError.self) {
            try await second.value
        }
        _ = try? await first.value
    }

    @Test("cancelling one of two requests for a load leaves the load running")
    func cancelledRequestLeavesTheSharedFetchRunning() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let gate = FetchGate()
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in
            await gate.pass()
            try Task.checkCancellation()
            return bytes
        })
        let content = ImageContent.libraryImage(coverRef(version: "1"))
        let executor = SerialTaskExecutor()

        let (first, second) = await withTaskExecutorPreference(executor) {
            let first = Task(executorPreference: executor) {
                try await store.image(content, pointSize: 56, displayScale: 2)
            }
            let second = Task(executorPreference: executor) {
                try await store.image(content, pointSize: 56, displayScale: 2)
            }
            await Task.yield()
            return (first, second)
        }

        first.cancel()
        await gate.open()
        #expect(try await second.value != nil)
        _ = try? await first.value
    }

    /// A failed fetch leaves nothing behind — no cached failure, no load still
    /// registered in flight — so asking again goes back to the source.
    @Test(
        "a failed load is asked again, not answered from a remembered failure"
    )
    func failedLoadIsAskedAgain() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let fetchCount = FetchCount()
        let store = ImageStore(fetchRemoteImage: { _, _ in
            await fetchCount.increment()
            if await fetchCount.read() == 1 {
                throw URLError(.badServerResponse)
            }
            return bytes
        })
        let content = ImageContent.remote(
            BridgeRemoteImageSet(
                url: "https://images.example/front.jpg",
                downscaled: []
            )
        )

        await #expect(throws: URLError.self) {
            try await store.image(content, pointSize: 56, displayScale: 2)
        }
        #expect(
            try await store.image(content, pointSize: 56, displayScale: 2)
                != nil
        )
        #expect(await fetchCount.read() == 2)
    }

    /// What a screenshot waits on before it captures the art it shows.
    @Test("an image counts as unanswered from its ask until it is handed back")
    @MainActor
    func unansweredImagesCountsLoadsUnderWay() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let gate = FetchGate()
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in
            await gate.pass()
            return bytes
        })
        let content = ImageContent.libraryImage(coverRef(version: "1"))
        #expect(store.unansweredImages == 0)

        let load = Task {
            try await store.image(content, pointSize: 56, displayScale: 2)
        }
        try await Wait.until { store.unansweredImages == 1 }
        await gate.open()
        #expect(try await load.value != nil)
        #expect(store.unansweredImages == 0)
    }
}

@Suite("ImageStore decoding")
struct ImageStoreDecodingTests {
    private let content = ImageContent.libraryImage(coverRef(version: "1"))

    /// Holds each decode that starts on a queue until `release`, and says
    /// when one has started.
    private final class DecodeHold: Sendable {
        private let started = DispatchSemaphore(value: 0)
        private let released = DispatchSemaphore(value: 0)

        func install(on queue: ImageDecodeQueue) {
            queue.beforeRun = { [started, released] in
                started.signal()
                released.wait()
            }
        }

        func waitUntilStarted() async {
            await withCheckedContinuation { continuation in
                DispatchQueue.global()
                    .async { [started] in
                        started.wait()
                        continuation.resume()
                    }
            }
        }

        func release() {
            released.signal()
        }
    }

    @Test("every decode runs on the decode queue, off Swift's shared threads")
    func decodesRunOnTheDecodeQueue() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let queue = ImageDecodeQueue(width: 2)
        let store = ImageStore(
            fetchLibraryImageBytes: { _, _ in bytes },
            decodeQueue: queue
        )

        _ = try await store.image(content, pointSize: 56, displayScale: 2)

        #expect(queue.runs.count == 1)
        #expect(queue.runs.allSatisfy { $0.onQueue && !$0.onMainThread })
    }

    @Test(
        "a decode of a picture of the import list runs on the decode queue too"
    )
    func localFileDecodesRunOnTheDecodeQueue() async throws {
        let directory = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("candidate.png")
        let bytes = try makePngBytes(width: 8, height: 8)
        try bytes.write(to: file)
        let queue = ImageDecodeQueue(width: 2)
        let store = ImageStore(
            fetchPicture: { _, _, _ in bytes },
            decodeQueue: queue
        )

        _ = try await store.image(
            .localFile(
                BridgeFileVersion(
                    path: file.pathBytes,
                    size: UInt64(bytes.count),
                    modifiedAtNs: 1_000
                )
            ),
            pointSize: 56,
            displayScale: 2
        )

        #expect(queue.runs.count == 1)
        #expect(queue.runs.allSatisfy { $0.onQueue && !$0.onMainThread })
    }

    @Test("no more decodes start than the queue's width, until one ends")
    func decodesRunAtMostWidthAtOnce() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let queue = ImageDecodeQueue(width: 2)
        let hold = DecodeHold()
        hold.install(on: queue)
        let store = ImageStore(
            fetchLibraryImageBytes: { _, _ in bytes },
            decodeQueue: queue
        )
        let loads = (0..<8)
            .map { index in
                Task {
                    try await store.image(
                        .libraryImage(coverRef(version: "v\(index)")),
                        pointSize: 56,
                        displayScale: 2
                    )
                }
            }

        await hold.waitUntilStarted()
        await hold.waitUntilStarted()
        #expect(queue.runs.count == 2, "two decodes are held; no third started")

        for _ in loads { hold.release() }
        for load in loads { _ = try await load.value }
        #expect(queue.runs.count == 8)
    }

    /// The last request for a decode that is under way is cancelled, and the
    /// same image is asked for again: the second request takes the running
    /// decode instead of starting another.
    @Test("a request after the last one was cancelled joins the running decode")
    func requestAfterCancelJoinsTheRunningDecode() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let queue = ImageDecodeQueue(width: 2)
        let hold = DecodeHold()
        hold.install(on: queue)
        let fetches = Counter()
        let store = ImageStore(
            fetchLibraryImageBytes: { _, _ in
                fetches.increment()
                return bytes
            },
            decodeQueue: queue
        )
        let content = content

        let first = Task {
            try await store.image(content, pointSize: 56, displayScale: 2)
        }
        await hold.waitUntilStarted()
        first.cancel()
        let second = Task {
            try await store.image(content, pointSize: 56, displayScale: 2)
        }
        hold.release()

        #expect(try await second.value != nil)
        _ = try? await first.value
        #expect(queue.runs.count == 1)
        #expect(fetches.count == 1)
    }

    /// A decode that has not started when its last request leaves is dropped:
    /// the request that follows starts the one decode.
    @Test(
        "cancelling the last request before its decode starts drops the decode"
    )
    @MainActor
    func cancelBeforeTheDecodeStartsDropsIt() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let queue = ImageDecodeQueue(width: 1)
        let hold = DecodeHold()
        hold.install(on: queue)
        let store = ImageStore(
            fetchLibraryImageBytes: { _, _ in bytes },
            decodeQueue: queue
        )
        let blocker = Task {
            try await store.image(
                .libraryImage(coverRef(version: "blocker")),
                pointSize: 56,
                displayScale: 2
            )
        }
        await hold.waitUntilStarted()
        let content = content

        let waiting = Task {
            try await store.image(content, pointSize: 56, displayScale: 2)
        }
        try await Wait.until { store.unansweredImages == 2 }
        waiting.cancel()
        await #expect(throws: CancellationError.self) {
            try await waiting.value
        }
        hold.release()
        _ = try await blocker.value

        // Nothing waits for the dropped decode, so the next request runs it.
        hold.release()
        #expect(
            try await store.image(content, pointSize: 56, displayScale: 2)
                != nil
        )
        #expect(queue.runs.count == 2)
    }
}

@Suite("ImageStore content identity")
struct ImageStoreContentIdentityTests {
    @Test("a new content version misses the previous version's decode")
    func versionBumpMisses() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        let store = ImageStore(fetchLibraryImageBytes: { _, _ in bytes })

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
        let store = ImageStore(fetchReleaseImageBytes: { _, _, _ in bytes })
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

    @Test(
        "an unchanged file version is served from the cache without asking core again"
    )
    func unchangedLocalFileHitsTheCache() async throws {
        let directory = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("candidate.png")
        let bytes = try makePngBytes(width: 8, height: 8)
        try bytes.write(to: file)
        let version = BridgeFileVersion(
            path: file.pathBytes,
            size: UInt64(bytes.count),
            modifiedAtNs: 1_000
        )

        let asked = Counter()
        let store = ImageStore(fetchPicture: { _, _, _ in
            asked.increment()
            return bytes
        })
        let content = ImageContent.localFile(version)
        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2) == nil
        )
        let loaded = try #require(
            try await store.image(content, pointSize: 56, displayScale: 2)
        )
        #expect(asked.count == 1)

        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2)
                === loaded
        )
        #expect(
            try await store.image(content, pointSize: 56, displayScale: 2)
                === loaded
        )
        #expect(asked.count == 1, "the second draw is the cached decode")
    }

    @Test("a new modification time at the same path is a new decode")
    func changedModificationTimeMissesTheCache() async throws {
        let directory = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("candidate.png")
        let bytes = try makePngBytes(width: 8, height: 8)
        try bytes.write(to: file)
        let scanned = BridgeFileVersion(
            path: file.pathBytes,
            size: UInt64(bytes.count),
            modifiedAtNs: 1_000
        )
        let rescanned = BridgeFileVersion(
            path: file.pathBytes,
            size: UInt64(bytes.count),
            modifiedAtNs: 2_000
        )

        let store = ImageStore(fetchPicture: { _, _, _ in bytes })
        let original = try #require(
            try await store.image(
                .localFile(scanned),
                pointSize: 56,
                displayScale: 2
            )
        )

        #expect(
            store.cachedImage(
                .localFile(rescanned),
                pointSize: 56,
                displayScale: 2
            ) == nil,
            "the rescanned version names new content"
        )
        let replaced = try #require(
            try await store.image(
                .localFile(rescanned),
                pointSize: 56,
                displayScale: 2
            )
        )
        #expect(replaced !== original)
        #expect(
            store.cachedImage(
                .localFile(rescanned),
                pointSize: 56,
                displayScale: 2
            ) === replaced
        )
    }

    @Test("embedded artwork is fetched once per source file version")
    func embeddedCoverIsFetchedOncePerVersion() async throws {
        let bytes = try makePngBytes(width: 8, height: 8)
        // No file exists at this path: the store never looks at it.
        let source = BridgeFileVersion(
            path: Data("/nonexistent/candidate/01.flac".utf8),
            size: 1_000,
            modifiedAtNs: 1_000
        )
        let fetches = Counter()
        let store = ImageStore(fetchPicture: { source, _, _ in
            guard
                case .embedded(_, let candidateKey, let sourceFileId) =
                    source
            else {
                Issue.record("embedded artwork asked for as \(source)")
                throw ImageStoreUnavailable()
            }
            #expect(candidateKey == Data("/candidate".utf8))
            #expect(sourceFileId == "01.flac")
            fetches.increment()
            return bytes
        })
        let content = ImageContent.embeddedCover(
            source: source,
            candidateKey: Data("/candidate".utf8),
            sourceFileId: "01.flac"
        )

        let loaded = try #require(
            try await store.image(content, pointSize: 56, displayScale: 2)
        )
        #expect(
            store.cachedImage(content, pointSize: 56, displayScale: 2)
                === loaded
        )
        _ = try await store.image(content, pointSize: 56, displayScale: 2)
        #expect(fetches.count == 1, "the second draw is the cached decode")

        let retagged = ImageContent.embeddedCover(
            source: BridgeFileVersion(
                path: source.path,
                size: source.size,
                modifiedAtNs: 2_000
            ),
            candidateKey: Data("/candidate".utf8),
            sourceFileId: "01.flac"
        )
        #expect(
            store.cachedImage(retagged, pointSize: 56, displayScale: 2) == nil,
            "a new version of the source file names new artwork"
        )
        #expect(
            store.cachedImage(
                .localFile(source),
                pointSize: 56,
                displayScale: 2
            ) == nil,
            "the file's embedded artwork is not the file"
        )
    }
}

private func temporaryDirectory() throws -> URL {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(
        at: directory,
        withIntermediateDirectories: true
    )
    return directory
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
