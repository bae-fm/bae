import BaeKit
import Foundation
import Observation
import Testing

@testable import bae

/// What the viewer's tile source does when core cannot give a tile or the
/// levels.
struct PictureTileFailureTests {
    @Test("A tile that fails is shown once and no more tiles are asked for")
    @MainActor
    func failedTilesAreShownOnce() async throws {
        let reads = TileReads()
        let images = ImageStore(fetchPictureTile: { _, level, x, y in
            reads.started(
                PictureTileKey(level: Int(level), x: Int(x), y: Int(y))
            )
            throw ImageStoreUnavailable()
        })
        let shown = Counter()
        let source = PictureTileSource(
            picture: .remote(
                image: BridgeRemoteImageSet(
                    url: "https://images.example/front.png",
                    downscaled: []
                )
            ),
            images: images,
            levelsArrived: { _ in },
            tileArrived: { _ in },
            failed: { _ in shown.increment() }
        )
        source.request(PictureTileKey(level: 0, x: 0, y: 0))
        source.request(PictureTileKey(level: 0, x: 1, y: 0))
        try await Wait.until { shown.count >= 1 }

        let later = PictureTileKey(level: 0, x: 2, y: 0)
        source.request(later)
        #expect(!reads.started.contains(later))
        #expect(shown.count == 1)
    }

    @Test(
        "A failure is told with core's error, which says whether retrying can help"
    )
    @MainActor
    func failuresAreToldWithTheirCause() async throws {
        let told = FailuresTold()
        func failing(with error: any Error) -> PictureTileSource {
            PictureTileSource(
                picture: .remote(
                    image: BridgeRemoteImageSet(
                        url: "https://images.example/front.png",
                        downscaled: []
                    )
                ),
                images: ImageStore(fetchPictureTile: { _, _, _, _ in
                    throw error
                }),
                levelsArrived: { _ in },
                tileArrived: { _ in },
                failed: { failure in
                    told.told.append(failure)
                }
            )
        }
        // Core's own error says whether trying again can help; here it rules
        // a retry out.
        failing(
            with: BridgeError.Diagnostic(category: .internal, detail: "no tile")
        )
        .request(PictureTileKey(level: 0, x: 0, y: 0))
        try await Wait.until { told.told.count == 1 }
        // Any other error is a bug: nothing of core's to show, and no retry.
        failing(with: ImageStoreUnavailable())
            .request(PictureTileKey(level: 0, x: 0, y: 0))
        try await Wait.until { told.told.count == 2 }

        #expect(told.told[0].error != nil)
        #expect(!told.told[0].offersRetry)
        #expect(told.told[1] == .bug)
    }

    @Test("A read that was cancelled and then fails stops no other read")
    @MainActor
    func cancelledReadsDoNotFail() async throws {
        let reads = TileReads()
        let release = FetchGate()
        let images = ImageStore(fetchPictureTile: { _, level, x, y in
            reads.started(
                PictureTileKey(level: Int(level), x: Int(x), y: Int(y))
            )
            await release.pass()
            throw ImageStoreUnavailable()
        })
        let source = PictureTileSource(
            picture: .remote(
                image: BridgeRemoteImageSet(
                    url: "https://images.example/front.png",
                    downscaled: []
                )
            ),
            images: images,
            levelsArrived: { _ in },
            tileArrived: { _ in },
            failed: { _ in }
        )
        let key = PictureTileKey(level: 0, x: 0, y: 0)
        source.request(key)
        try await Wait.until { reads.started == [key] }

        source.keepOnly([])
        await release.open()
        let other = PictureTileKey(level: 0, x: 1, y: 0)
        source.request(other)
        // The cancelled read failing left no failure standing, so a read
        // for another tile starts.
        try await Wait.until { reads.started.contains(other) }
    }

    @Test("A levels read that was cancelled and then fails is not shown")
    @MainActor
    func cancelledLevelsReadsDoNotFail() async throws {
        let asks = TileReads()
        let release = FetchGate()
        let secondMayAnswer = FetchGate()
        let arrived = Counter()
        let images = ImageStore(fetchPictureTiles: { _ in
            let ask = asks.ask()
            if ask == 1 {
                await release.pass()
                throw ImageStoreUnavailable()
            }
            await secondMayAnswer.pass()
            return BridgePictureTiles(tileEdge: 512, levels: [])
        })
        let shown = Counter()
        let source = PictureTileSource(
            picture: .remote(
                image: BridgeRemoteImageSet(
                    url: "https://images.example/front.png",
                    downscaled: []
                )
            ),
            images: images,
            levelsArrived: { _ in arrived.increment() },
            tileArrived: { _ in },
            failed: { _ in shown.increment() }
        )
        source.readLevels()
        try await Wait.until { asks.asked == 1 }
        let first = try #require(source.levelsRead)
        source.close()
        source.readLevels()
        try await Wait.until { asks.asked == 2 }
        // The first read fails now, and is over once its task has ended.
        await release.open()
        await first.value
        await secondMayAnswer.open()
        try await Wait.until { arrived.count == 1 }

        #expect(shown.count == 0)
    }
}

@MainActor
@Observable
private final class FailuresTold {
    var told: [LightboxFailure] = []
}
