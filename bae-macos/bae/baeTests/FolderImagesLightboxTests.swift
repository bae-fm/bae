import BaeKit
import Foundation
import Testing

@testable import bae

/// The folder lightbox keeps only the file id the person moved to, and draws
/// the image and its neighbors that core's read answers for it.
@MainActor
struct FolderImagesLightboxTests {
    private let key = Data([1, 2, 3])

    @Test("The lightbox shows the image core's read focuses")
    func opensAtTheClickedImage() async throws {
        let folder = FakeFolder(names: ["a", "b", "c"])
        let lightbox = FolderImagesLightbox(
            candidateKey: key,
            focusedImage: "b",
            query: folder.query,
            onError: { _ in }
        )
        defer { lightbox.close() }

        try await Wait.until { lightbox.place?.current.id == "b" }
        #expect(lightbox.place?.items.map(\.id) == ["a", "b", "c"])
        #expect(
            lightbox.place?.neighbors
                == LightboxNeighbors(previous: "a", next: "c")
        )
    }

    @Test("Moving to an image asks core again")
    func movesThroughCore() async throws {
        let folder = FakeFolder(names: ["a", "b", "c"])
        let lightbox = FolderImagesLightbox(
            candidateKey: key,
            focusedImage: "a",
            query: folder.query,
            onError: { _ in }
        )
        defer { lightbox.close() }
        try await Wait.until { lightbox.place?.current.id == "a" }

        lightbox.focus("c")

        try await Wait.until { lightbox.place?.current.id == "c" }
        #expect(folder.requests.map(\.focus) == ["a", "c"])
    }

    /// A value that answers an earlier image arrives after the person moved
    /// on; the lightbox keeps drawing the image the person moved to.
    @Test("The lightbox skips a value for an image the person left")
    func skipsAnEarlierAnswer() async throws {
        let folder = FakeFolder(names: ["a", "b", "c"])
        let lightbox = FolderImagesLightbox(
            candidateKey: key,
            focusedImage: "a",
            query: folder.query,
            onError: { _ in }
        )
        defer { lightbox.close() }
        lightbox.focus("c")
        try await Wait.until { lightbox.place?.current.id == "c" }

        folder.answer(focus: "a")
        folder.names.append("d")
        folder.answer(focus: "c")

        try await Wait.until { lightbox.place?.items.count == 4 }
        #expect(lightbox.place?.current.id == "c")
    }
}

/// A candidate folder's images as a fake of core's read, which focuses the
/// image a request names and wraps its neighbors around the ends.
@MainActor
private final class FakeFolder {
    var names: [String]
    private(set) var requests: [BridgeFolderImagesRequest] = []
    private let key = Data([1, 2, 3])
    private let continuation:
        AsyncStream<BridgeFolderImagesSnapshot>.Continuation
    let query: FolderImagesQuery

    init(names: [String]) {
        self.names = names
        let (stream, continuation) = AsyncStream<BridgeFolderImagesSnapshot>
            .makeStream()
        self.continuation = continuation
        let pulls = FolderPulls(stream.makeAsyncIterator())
        let requested = Requested()
        query = FolderImagesQuery(
            setRequest: { request in
                MainActor.assumeIsolated { requested.handle?(request) }
            },
            values: PulledValues(
                read: { try await pulls.next() },
                cancel: { continuation.finish() }
            )
        )
        requested.handle = { [weak self] request in
            guard let self, let request else { return }
            self.requests.append(request)
            self.answer(focus: request.focus)
        }
    }

    /// Deliver the folder's images as core answers a request that focuses
    /// the image whose file id is `focus`.
    func answer(focus: String?) {
        let images = names.map { name in
            BridgeMappingImage(
                fileId: name,
                name: name,
                file: PreviewData.previewArtFile("Front.png")
            )
        }
        let index = images.firstIndex { $0.fileId == focus } ?? 0
        let count = images.count
        continuation.yield(
            BridgeFolderImagesSnapshot(
                request: BridgeFolderImagesRequest(
                    candidateKey: key,
                    focus: focus
                ),
                images: BridgeFolderImages(
                    images: images,
                    focus: BridgeFolderImageFocus(
                        image: images[index],
                        neighbors: count > 1
                            ? BridgeFolderImageNeighbors(
                                previous: images[(index + count - 1) % count]
                                    .fileId,
                                next: images[(index + 1) % count].fileId
                            )
                            : nil
                    )
                )
            )
        )
        Wait.happened()
    }
}

/// What the fake folder does with each request the lightbox sets.
private final class Requested: @unchecked Sendable {
    var handle: (@MainActor (BridgeFolderImagesRequest?) -> Void)?
}

/// The one reader of the fake folder's values.
private final class FolderPulls: @unchecked Sendable {
    private var iterator: AsyncStream<BridgeFolderImagesSnapshot>.AsyncIterator

    init(_ iterator: AsyncStream<BridgeFolderImagesSnapshot>.AsyncIterator) {
        self.iterator = iterator
    }

    func next() async throws -> BridgeFolderImagesSnapshot {
        guard let snapshot = await iterator.next() else {
            throw CancellationError()
        }
        return snapshot
    }
}
