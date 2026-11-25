import BaeKit
import CoreGraphics
import Foundation
import Testing

@testable import bae

struct PictureTilesTests {
    /// A 6000 by 4000 picture's levels, as core cuts them: halving from its
    /// own size until one tile holds it.
    private static let tiles = BridgePictureTiles(
        tileEdge: 512,
        levels: [
            level(6000, 4000),
            level(3000, 2000),
            level(1500, 1000),
            level(750, 500),
            level(375, 250),
        ]
    )

    private static func level(_ width: UInt32, _ height: UInt32)
        -> BridgeTileLevel
    {
        BridgeTileLevel(
            width: width,
            height: height,
            columns: (width + 511) / 512,
            rows: (height + 511) / 512
        )
    }

    /// The picture fitted to a stage 600 by 400 points large, over a copy
    /// decoded 1200 pixels wide.
    private let grid = PictureTileGrid(
        tiles: tiles,
        size: CGSize(width: 600, height: 400),
        copyPixels: 1200
    )

    @Test("A zoom draws the smallest level with a pixel for each one drawn")
    func levelMatchesTheScreensPixels() {
        // At rest on a Retina screen, 1200 pixels across: the 1500 level.
        #expect(grid.level(forScale: 2) == 2)
        // Zoomed in four times: 4800 pixels, which only the picture's own
        // size has.
        #expect(grid.level(forScale: 8) == 0)
        // Zoomed in twice: 2400 pixels, which the 3000 level covers.
        #expect(grid.level(forScale: 4) == 1)
        // Further in than the picture's own pixels go.
        #expect(grid.level(forScale: 64) == 0)
    }

    @Test("Only the levels finer than the fitted copy draw tiles")
    func levelsFinerThanTheCopy() {
        #expect(grid.drawnLevels == [2, 1, 0])
        let sharp = PictureTileGrid(
            tiles: Self.tiles,
            size: grid.size,
            copyPixels: 6000
        )
        #expect(sharp.drawnLevels == [])
    }

    @Test("A tile covers its share of the stage, the last ones cut short")
    func tileRects() {
        #expect(
            grid.rect(of: PictureTileKey(level: 0, x: 0, y: 0))
                == CGRect(x: 0, y: 0, width: 51.2, height: 51.2)
        )
        // The last column of the picture's own size holds 6000 - 11 × 512
        // = 368 pixels.
        let last = grid.rect(of: PictureTileKey(level: 0, x: 11, y: 7))
        #expect(abs(last.width - 36.8) < 0.001)
        #expect(abs(last.maxX - 600) < 0.001)
        #expect(abs(last.maxY - 400) < 0.001)
    }

    @Test("The tiles an area touches, and none beyond the picture")
    func keysInAnArea() {
        let keys = grid.keys(
            level: 0,
            in: CGRect(x: 50, y: 0, width: 60, height: 10)
        )
        #expect(
            keys == [0, 1, 2].map { PictureTileKey(level: 0, x: $0, y: 0) }
        )
        let edge = grid.keys(
            level: 4,
            in: CGRect(x: -100, y: -100, width: 1000, height: 1000)
        )
        #expect(edge == [PictureTileKey(level: 4, x: 0, y: 0)])
        #expect(
            grid.keys(level: 0, in: CGRect(x: 700, y: 0, width: 10, height: 10))
                .isEmpty
        )
    }

    @Test("A tile that leaves the screen stops its read, and closing stops all")
    @MainActor
    func leavingTilesStopTheirReads() async throws {
        let reads = TileReads()
        let images = ImageStore(fetchPictureTile: { _, level, x, y in
            await reads.started(
                PictureTileKey(level: Int(level), x: Int(x), y: Int(y))
            )
            do {
                try await Task.neverAnswers()
            }
            catch {
                await reads.stopped(
                    PictureTileKey(level: Int(level), x: Int(x), y: Int(y))
                )
                throw error
            }
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
        let kept = PictureTileKey(level: 0, x: 0, y: 0)
        let left = PictureTileKey(level: 0, x: 1, y: 0)
        source.request(kept)
        source.request(left)
        try await Wait.until { await reads.started.count == 2 }

        source.keepOnly([kept])
        try await Wait.until { await reads.stopped == [left] }

        source.close()
        try await Wait.until { await reads.stopped == [left, kept] }
    }

    @Test("An arrived tile is drawn from memory and not asked for again")
    @MainActor
    func arrivedTilesAreKept() async throws {
        let png = try makePngBytes(width: 512, height: 512)
        let reads = TileReads()
        let images = ImageStore(fetchPictureTile: { _, level, x, y in
            await reads.started(
                PictureTileKey(level: Int(level), x: Int(x), y: Int(y))
            )
            return png
        })
        let arrived = ArrivedTiles()
        let source = PictureTileSource(
            picture: .remote(
                image: BridgeRemoteImageSet(
                    url: "https://images.example/front.png",
                    downscaled: []
                )
            ),
            images: images,
            levelsArrived: { _ in },
            tileArrived: { arrived.keys.append($0) },
            failed: { _ in }
        )
        let key = PictureTileKey(level: 2, x: 1, y: 1)
        source.request(key)
        try await Wait.until { arrived.keys == [key] }
        #expect(source.tile(key)?.width == 512)

        source.request(key)
        source.keepOnly([key])
        #expect(await reads.started == [key])
    }
}

actor TileReads {
    private(set) var asked = 0
    private(set) var started: [PictureTileKey] = []
    private(set) var stopped: [PictureTileKey] = []

    /// The number of this ask of the levels, counted from 1.
    func ask() -> Int {
        asked += 1
        Wait.happened()
        return asked
    }

    func started(_ key: PictureTileKey) {
        started.append(key)
        Wait.happened()
    }

    func stopped(_ key: PictureTileKey) {
        stopped.append(key)
        Wait.happened()
    }
}

@MainActor
private final class ArrivedTiles {
    var keys: [PictureTileKey] = [] {
        didSet { Wait.happened() }
    }
}
