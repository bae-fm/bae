import BaeKit
import Foundation

@testable import bae

/// One feed of values a preview list answers a read with: the preview emits,
/// the read takes them in order, and a failure ends the feed.
final class PreviewQueueFeed<Value: Sendable>: @unchecked Sendable {
    private let continuation: AsyncThrowingStream<Value, any Error>.Continuation
    private var iterator: AsyncThrowingStream<Value, any Error>.AsyncIterator

    init() {
        let (stream, continuation) = AsyncThrowingStream<Value, any Error>
            .makeStream()
        self.continuation = continuation
        iterator = stream.makeAsyncIterator()
    }

    func emit(_ value: Value) { continuation.yield(value) }

    func fail(_ error: any Error) { continuation.finish(throwing: error) }

    func next() async throws -> Value {
        guard let value = try await iterator.next() else {
            throw CancellationError()
        }
        return value
    }
}

/// A queue's context lane read as the list core serves: windows bounded by
/// keys, a count, the position of a key and the key at a position. A row's
/// key is its place in the lane, which a change renumbers densely as core's lane does, and every
/// change reaches every open window and the count, as core's live reads do.
final class PreviewQueueList: @unchecked Sendable {
    private final class Window: @unchecked Sendable {
        let feed = PreviewQueueFeed<ListWindowDelivery<BridgeQueueEntry>>()
        var request: BridgeListWindow
        var stopped = false

        init(_ request: BridgeListWindow) { self.request = request }
    }

    private let lock = NSLock()
    private var rows: [BridgeQueueEntry] = []
    private var open: [Window] = []
    private let counts = PreviewQueueFeed<BridgeListCount>()
    /// Why every read of the list fails, when it does.
    private let failure: (any Error)?

    init(failure: (any Error)? = nil) { self.failure = failure }

    /// Replace the lane's rows and deliver the change to the count and every
    /// window.
    func show(_ rows: [BridgeQueueEntry]) {
        let windows = lock.withLock {
            self.rows = rows
            return open.filter { !$0.stopped }
        }
        counts.emit(
            BridgeListCount(
                rows: UInt64(rows.count),
                items: UInt64(rows.count),
                extra: .none
            )
        )
        for window in windows { deliver(to: window) }
    }

    /// Fail every open window's read with `error`, as a later read of a live
    /// list fails.
    func failWindows(_ error: any Error) {
        let windows = lock.withLock { open.filter { !$0.stopped } }
        for window in windows { window.feed.fail(error) }
    }

    private static func key(_ place: Int) -> [BridgeSortValue] {
        [.integer(value: Int64(place))]
    }

    private static func place(_ key: [BridgeSortValue]) -> Int {
        guard case .integer(let place)? = key.first else { return 0 }
        return Int(place)
    }

    private func delivery(of window: BridgeListWindow) -> ListWindowDelivery<
        BridgeQueueEntry
    > {
        lock.withLock {
            let start: Int
            let stop: Int
            let beyond: Int?
            switch window {
            case .forward(let from, let until, let limit):
                start = min(from.map(Self.place) ?? 0, rows.count)
                let end = min(until.map(Self.place) ?? rows.count, rows.count)
                stop = max(start, min(start + Int(limit), end))
                beyond = stop < end ? stop : nil
            case .backward(let until, let limit):
                stop = min(Self.place(until), rows.count)
                start = max(0, stop - Int(limit))
                beyond = start > 0 ? start - 1 : nil
            }
            let held = Array(rows[start..<stop])
            return ListWindowDelivery(
                window: window,
                rows: held,
                first: held.isEmpty ? nil : Self.key(start),
                beyond: beyond.map(Self.key)
            )
        }
    }

    private func deliver(to window: Window) {
        window.feed.emit(delivery(of: window.request))
    }

    var queries: BridgeListQueries<BridgeQueueEntry> {
        BridgeListQueries(
            window: { [self] first in
                let window = Window(first)
                lock.withLock { open.append(window) }
                if let failure {
                    window.feed.fail(failure)
                }
                else {
                    deliver(to: window)
                }
                return ListWindowQuery(
                    setWindow: { [self] next in
                        window.request = next
                        deliver(to: window)
                    },
                    values: PulledValues(
                        read: { try await window.feed.next() },
                        cancel: { window.stopped = true }
                    )
                )
            },
            count: { [self] in
                if let failure {
                    counts.fail(failure)
                }
                else {
                    let count = UInt64(lock.withLock { rows.count })
                    counts.emit(
                        BridgeListCount(rows: count, items: count, extra: .none)
                    )
                }
                return PulledValues(read: { try await self.counts.next() })
            },
            position: { [self] key in
                let feed = PreviewQueueFeed<BridgeListPosition>()
                let emit: @Sendable ([BridgeSortValue]) -> Void = { key in
                    feed.emit(
                        BridgeListPosition(
                            key: key,
                            rowsBefore: UInt64(Self.place(key))
                        )
                    )
                }
                emit(key)
                return ListPositionQuery(
                    setKey: emit,
                    values: PulledValues(read: { try await feed.next() })
                )
            },
            keyAt: { [self] position in
                lock.withLock {
                    rows.isEmpty
                        ? nil
                        : Self.key(min(Int(position), rows.count - 1))
                }
            }
        )
    }
}

extension PreviewData {
    // MARK: - Queue

    static let queueEntries: [BridgeQueueEntry] = [
        BridgeQueueEntry(
            entryId: "e-01",
            trackId: "t-01",
            display: BridgeTrackDisplay(
                title: "Track Title 1",
                artistLine: .named("Artist Name A"),
                albumId: "a-01",
                releaseId: "r-01",
                albumTitle: "Album Title A",
                coverImage: nil
            ),
            durationClock: bridgeLengthClock(ms: 210_000)
        ),
        BridgeQueueEntry(
            entryId: "e-02",
            trackId: "t-02",
            display: BridgeTrackDisplay(
                title: "Track Title 2",
                artistLine: .named("Artist Name A"),
                albumId: "a-01",
                releaseId: "r-01",
                albumTitle: "Album Title A",
                coverImage: nil
            ),
            durationClock: bridgeLengthClock(ms: 240_000)
        ),
        BridgeQueueEntry(
            entryId: "e-03",
            trackId: "t-03",
            display: BridgeTrackDisplay(
                title: "Track Title 3",
                artistLine: .named("Artist Name B"),
                albumId: "a-02",
                releaseId: "r-02",
                albumTitle: "Album Title B",
                coverImage: nil
            ),
            durationClock: bridgeLengthClock(ms: 198_000)
        ),
        BridgeQueueEntry(
            entryId: "e-04",
            trackId: "t-04",
            display: BridgeTrackDisplay(
                title: "Track Title 4",
                artistLine: .named("Artist Name B"),
                albumId: "a-02",
                releaseId: "r-02",
                albumTitle: "Album Title B",
                coverImage: nil
            ),
            durationClock: bridgeLengthClock(ms: 225_000)
        ),
        BridgeQueueEntry(
            entryId: "e-05",
            trackId: "t-05",
            display: BridgeTrackDisplay(
                title: "Track Title 5",
                artistLine: .named("Artist Name C"),
                albumId: "a-03",
                releaseId: "r-03",
                albumTitle: "Album Title C",
                coverImage: nil
            ),
            durationClock: bridgeLengthClock(ms: 187_000)
        ),
    ]

    /// A `PlaybackStore` preloaded with `queueEntries`: `manualCount` in the
    /// manual lane, and a context of `context`'s kind, for shots that show
    /// the store's queue state but not the context's rows.
    @MainActor
    static func queueStore(
        manualCount: Int,
        context: BridgePlaybackSourceKind? = .release,
        shuffled: Bool = false,
        trackPositions: LatestStream<BridgeTrackPosition> =
            .trackPositions()
    ) -> PlaybackStore {
        let store = PlaybackStore(trackPositions: trackPositions)
        let manual = Array(queueEntries.prefix(manualCount))
        let upcoming = Array(queueEntries.suffix(from: manualCount))
        store.applyQueueSnapshot(
            BridgeQueueSnapshot(
                manual: manual,
                context: context.map { kind in
                    BridgePlaybackContext(
                        kind: kind,
                        sourceTitle: kind == .release
                            ? "Neon Frequencies" : nil
                    )
                },
                shuffled: shuffled,
                hasNext: !upcoming.isEmpty,
                hasPrevious: false,
                upNextCount: UInt32(manual.count),
                isEmpty: manual.isEmpty && (context == nil || upcoming.isEmpty)
            )
        )
        return store
    }
}

extension PreviewData {
    // MARK: - Cover sheet

    /// Remote cover candidates (front + back) for the CoverSheetView shot.
    static let remoteCovers: [BridgeRemoteCover] = [
        remoteCover(
            url: "https://example.com/cover1.jpg",
            downscaledUrl: "https://example.com/cover1-250.jpg",
            label: .lead(releaseGroup: false)
        ),
        remoteCover(
            url: "https://example.com/cover2.jpg",
            downscaledUrl: "https://example.com/cover2-250.jpg",
            label: .gallery(
                reference: nil,
                position: 2,
                types: [.back],
                comment: ""
            )
        ),
    ]

    private static func remoteCover(
        url: String,
        downscaledUrl: String,
        label: BridgeCoverLabel
    ) -> BridgeRemoteCover {
        let image = BridgeRemoteImageSet(
            url: url,
            downscaled: [
                BridgeDownscaledCopy(url: downscaledUrl, maxEdge: 250)
            ]
        )
        return BridgeRemoteCover(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: .musicBrainz
                    )
                ),
                image: .remote(image: image)
            ),
            sourceName: "Cover Art Archive",
            label: label
        )
    }
}
