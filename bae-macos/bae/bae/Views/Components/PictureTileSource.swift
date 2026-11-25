import BaeKit
import CoreGraphics
import Foundation
import OSLog

private let logger = Logger.bae("PictureTiles")

/// One tile of a picture: column `x` and row `y` of level `level`, 0 being
/// the picture's own size.
struct PictureTileKey: Hashable, Sendable {
    let level: Int
    let x: Int
    let y: Int
}

/// Where a picture's tiles fall on a view that shows the whole picture
/// `size` points large, before any zoom, over the copy fitted to it,
/// `copyPixels` wide.
struct PictureTileGrid: Equatable {
    let tiles: BridgePictureTiles
    let size: CGSize
    let copyPixels: Int

    /// The level a view draws at `scale` pixels per point: the smallest one
    /// with a pixel for each pixel drawn, or the picture's own size when
    /// none has.
    func level(forScale scale: CGFloat) -> Int {
        let needed = size.width * scale
        var chosen = 0
        for (index, level) in tiles.levels.enumerated()
        where CGFloat(level.width) >= needed.rounded(.down) {
            chosen = index
        }
        return chosen
    }

    /// The levels finer than the fitted copy, coarsest first: the ones whose
    /// tiles show more than the copy does.
    var drawnLevels: [Int] {
        tiles.levels.indices.reversed()
            .filter {
                Int(tiles.levels[$0].width) > copyPixels
            }
    }

    /// The area `key` covers, in the view's points.
    func rect(of key: PictureTileKey) -> CGRect {
        let level = tiles.levels[key.level]
        let edge = Int(tiles.tileEdge)
        let left = key.x * edge
        let top = key.y * edge
        let width = min(edge, Int(level.width) - left)
        let height = min(edge, Int(level.height) - top)
        let across = size.width / CGFloat(level.width)
        let down = size.height / CGFloat(level.height)
        return CGRect(
            x: CGFloat(left) * across,
            y: CGFloat(top) * down,
            width: CGFloat(width) * across,
            height: CGFloat(height) * down
        )
    }

    /// The tiles of `level` that `rect`, in the view's points, touches.
    func keys(level index: Int, in rect: CGRect) -> [PictureTileKey] {
        let level = tiles.levels[index]
        let area = rect.intersection(CGRect(origin: .zero, size: size))
        guard !area.isNull, !area.isEmpty else { return [] }
        let edge = CGFloat(tiles.tileEdge)
        let across = CGFloat(level.width) / size.width / edge
        let down = CGFloat(level.height) / size.height / edge
        let columns = Int(level.columns) - 1
        let rows = Int(level.rows) - 1
        let first = (
            x: min(columns, Int((area.minX * across).rounded(.down))),
            y: min(rows, Int((area.minY * down).rounded(.down)))
        )
        let last = (
            x: min(columns, Int((area.maxX * across).rounded(.up)) - 1),
            y: min(rows, Int((area.maxY * down).rounded(.up)) - 1)
        )
        guard first.x <= last.x, first.y <= last.y else { return [] }
        return (first.y...last.y)
            .flatMap { y in
                (first.x...last.x)
                    .map { x in
                        PictureTileKey(level: index, x: x, y: y)
                    }
            }
    }
}

/// The tiles of one picture a viewer zooms: core's levels once a zoom asks
/// for them, and the decoded tiles the viewer draws, a few screens' worth,
/// fetched as they come on screen. Drawing threads read it, so a lock
/// guards its state.
final class PictureTileSource: @unchecked Sendable {
    /// What the decoded tiles take in memory at most: about four screens of
    /// a large display.
    private static let tileByteBudget = 64 * 1024 * 1024

    /// One read of a tile, which only the read that still holds its place in
    /// `reads` may change state for.
    private final class TileRead: @unchecked Sendable {
        var task: Task<Void, Never>?
    }

    private let picture: BridgeCoverImageSource
    private let images: ImageStore
    /// Runs on the main actor when the levels arrive.
    private let levelsArrived: @MainActor @Sendable (BridgePictureTiles) -> Void
    /// Runs on the main actor when a tile arrives.
    private let tileArrived: @MainActor @Sendable (PictureTileKey) -> Void
    /// Runs on the main actor when core could not give the levels or a tile.
    private let failed:
        @MainActor @Sendable (_ failure: LightboxFailure) -> Void

    private let lock = NSLock()
    private var levels: BridgePictureTiles?
    private(set) var levelsRead: Task<Void, Never>?
    private var reads: [PictureTileKey: TileRead] = [:]
    /// Whether the picture's levels or a tile failed, which the viewer shows
    /// and asks again by making a source of its own.
    private var hasFailed = false
    private let decoded = NSCache<NSString, CGImage>()

    init(
        picture: BridgeCoverImageSource,
        images: ImageStore,
        levelsArrived:
            @escaping @MainActor @Sendable (BridgePictureTiles) ->
            Void,
        tileArrived: @escaping @MainActor @Sendable (PictureTileKey) -> Void,
        failed:
            @escaping @MainActor @Sendable (_ failure: LightboxFailure) -> Void
    ) {
        self.picture = picture
        self.images = images
        self.levelsArrived = levelsArrived
        self.tileArrived = tileArrived
        self.failed = failed
        decoded.totalCostLimit = Self.tileByteBudget
    }

    /// Whether tiles are on their way.
    var isReading: Bool {
        lock.withLock { levelsRead != nil || !reads.isEmpty }
    }

    /// Ask core for the picture's levels, which cuts its tiles the first
    /// time: once, and not again after a failure.
    func readLevels() {
        lock.withLock {
            guard levels == nil, levelsRead == nil, !hasFailed else { return }
            levelsRead = Task { [self] in
                do {
                    let tiles = try await images.pictureTiles(picture)
                    // The bridge call does not throw on a cancel, so a viewer
                    // that closed meanwhile is told here.
                    try Task.checkCancellation()
                    lock.withLock {
                        levels = tiles
                        levelsRead = nil
                    }
                    await levelsArrived(tiles)
                }
                catch is CancellationError {
                    return
                }
                catch {
                    // A viewer that closed meanwhile has nothing to say.
                    guard !Task.isCancelled else { return }
                    lock.withLock { levelsRead = nil }
                    await fail(error)
                }
            }
        }
    }

    /// The tile `key`, once it arrived and while memory keeps it.
    func tile(_ key: PictureTileKey) -> CGImage? {
        decoded.object(forKey: Self.name(key))
    }

    /// Fetch the tile `key` unless it is here or on its way, or a failure
    /// stopped the reads.
    func request(_ key: PictureTileKey) {
        if tile(key) != nil { return }
        lock.withLock {
            guard reads[key] == nil, !hasFailed else { return }
            let read = TileRead()
            reads[key] = read
            read.task = Task { [self] in
                await self.read(key, as: read)
            }
        }
    }

    /// Keep fetching only the tiles in `wanted`, and fetch those of them
    /// that are missing: the viewer moved, so the rest no longer show.
    func keepOnly(_ wanted: Set<PictureTileKey>) {
        lock.withLock {
            for (key, read) in reads where !wanted.contains(key) {
                read.task?.cancel()
                reads.removeValue(forKey: key)
            }
        }
        for key in wanted {
            request(key)
        }
    }

    /// The viewer closed: stop every read, which stops core's cut when no
    /// other viewer wants the picture.
    func close() {
        lock.withLock {
            levelsRead?.cancel()
            levelsRead = nil
            for read in reads.values {
                read.task?.cancel()
            }
            reads.removeAll()
        }
    }

    private func read(_ key: PictureTileKey, as read: TileRead) async {
        do {
            let tile = try await images.pictureTile(
                picture,
                level: UInt32(key.level),
                x: UInt32(key.x),
                y: UInt32(key.y)
            )
            try Task.checkCancellation()
            decoded.setObject(
                tile,
                forKey: Self.name(key),
                cost: tile.width * tile.height * 4
            )
            lock.withLock { endRead(of: key, read) }
            await tileArrived(key)
        }
        catch is CancellationError {
            return
        }
        catch {
            // A read that was cancelled meanwhile has nothing to say, and
            // its place may be a newer read's.
            guard !Task.isCancelled else { return }
            lock.withLock { endRead(of: key, read) }
            await fail(error)
        }
    }

    /// Take `read` out of `reads` when it still holds `key`'s place. Called
    /// under the lock.
    private func endRead(of key: PictureTileKey, _ read: TileRead) {
        if reads[key] === read {
            reads.removeValue(forKey: key)
        }
    }

    /// Stop asking and tell the viewer, once. An error that is not core's
    /// own is a bug, which goes to the bug report too.
    private func fail(_ error: any Error) async {
        let first = lock.withLock {
            let first = !hasFailed
            hasFailed = true
            return first
        }
        guard first else { return }
        if error is BridgeError {
            logger.warning(
                "No tiles of \(String(describing: self.picture)): \(String(reflecting: error))"
            )
        }
        await failed(LightboxFailure(of: error, operation: "zoom a picture"))
    }

    private static func name(_ key: PictureTileKey) -> NSString {
        "\(key.level)/\(key.x)_\(key.y)" as NSString
    }
}

extension PictureTileSource {
    /// Draw the tiles of `grid` that `rect` touches at `scale` pixels per
    /// point into `context`, whose space is the view's points from the top
    /// left: the coarser levels' tiles first, so a tile still on its way
    /// shows the best one here, and those of the level `scale` needs last.
    /// Asks for that level's missing tiles when `requesting`. Draws nothing
    /// where the fitted copy shows as much.
    func draw(
        _ grid: PictureTileGrid,
        in context: CGContext,
        rect: CGRect,
        scale: CGFloat,
        requesting: Bool
    ) {
        let wanted = grid.level(forScale: scale)
        for level in grid.drawnLevels where level >= wanted {
            for key in grid.keys(level: level, in: rect) {
                if let tile = tile(key) {
                    drawUpright(tile, in: grid.rect(of: key), context: context)
                }
                else if requesting, level == wanted {
                    request(key)
                }
            }
        }
    }
}

/// Draw `image` upright into `rect` of `context`, whose space runs down from
/// the top left.
func drawUpright(_ image: CGImage, in rect: CGRect, context: CGContext) {
    context.saveGState()
    context.translateBy(x: 0, y: rect.minY + rect.maxY)
    context.scaleBy(x: 1, y: -1)
    context.interpolationQuality = .high
    context.draw(image, in: rect)
    context.restoreGState()
}
