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

/// This list serves a queue's context lane the way core serves it: a view
/// that answers the rows on screen with the rows around them, and a count.
/// Every change reaches every open view and the count, as core's live reads
/// do.
final class PreviewQueueList: @unchecked Sendable {
    private final class View: @unchecked Sendable {
        let feed = PreviewQueueFeed<ListViewDelivery<BridgeQueueEntry>>()
        var shown: ClosedRange<Int>
        var stopped = false

        init(_ shown: ClosedRange<Int>) { self.shown = shown }
    }

    private let lock = NSLock()
    private var rows: [BridgeQueueEntry] = []
    private var open: [View] = []
    private let counts = PreviewQueueFeed<BridgeListCount>()
    /// Why every read of the list fails, when it does.
    private let failure: (any Error)?

    init(failure: (any Error)? = nil) { self.failure = failure }

    /// Replace the lane's rows and deliver the change to the count and every
    /// view.
    func show(_ rows: [BridgeQueueEntry]) {
        let views = lock.withLock {
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
        for view in views { deliver(to: view) }
    }

    /// Fail every open view's read with `error`, as a later read of a live
    /// list fails.
    func failWindows(_ error: any Error) {
        let views = lock.withLock { open.filter { !$0.stopped } }
        for view in views { view.feed.fail(error) }
    }

    private func deliver(to view: View) {
        let delivery = lock.withLock {
            BridgeListQueries<BridgeQueueEntry>
                .fixedDelivery(
                    first: view.shown.lowerBound,
                    last: view.shown.upperBound,
                    rows: rows
                )
        }
        view.feed.emit(delivery)
    }

    var queries: BridgeListQueries<BridgeQueueEntry> {
        BridgeListQueries(
            view: { [self] first, last in
                let view = View(Int(first)...Int(last))
                lock.withLock { open.append(view) }
                if let failure {
                    view.feed.fail(failure)
                }
                else {
                    deliver(to: view)
                }
                return ListViewQuery(
                    setVisible: { [self] first, last in
                        view.shown = Int(first)...Int(last)
                        deliver(to: view)
                    },
                    values: PulledValues(
                        read: { try await view.feed.next() },
                        cancel: { view.stopped = true }
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
            durationClock: BridgeDurationClock.length(ms: 210_000)
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
            durationClock: BridgeDurationClock.length(ms: 240_000)
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
            durationClock: BridgeDurationClock.length(ms: 198_000)
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
            durationClock: BridgeDurationClock.length(ms: 225_000)
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
            durationClock: BridgeDurationClock.length(ms: 187_000)
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

    static func musicBrainzCoverChoice(_ image: BridgeRemoteImageSet)
        -> BridgeCoverChoice
    {
        BridgeCoverChoice(
            selection: .remoteCover(
                selection: BridgeRemoteCoverSelection(
                    image: image,
                    source: .musicBrainz
                )
            ),
            image: .remote(image: image)
        )
    }

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
            coverChoice: musicBrainzCoverChoice(image),
            sourceName: "Cover Art Archive",
            label: label
        )
    }
}
