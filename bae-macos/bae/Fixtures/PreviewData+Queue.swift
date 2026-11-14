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

/// The upcoming list behind `PreviewData.echoingQueue`: the model's context
/// lane read as the list core serves, windows bounded by keys, a count, the
/// position of a key and the key at a position. A row's key is its place in
/// the lane, which a change renumbers densely as core's lane does, and every
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
        counts.emit(BridgeListCount(rows: UInt64(rows.count), extra: .none))
        for window in windows { deliver(to: window) }
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
                    counts.emit(
                        BridgeListCount(
                            rows: UInt64(lock.withLock { rows.count }),
                            extra: .none
                        )
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

/// A queue shot: the store the queue pane reads, and the `Queue` whose
/// commands and upcoming read answer out of an in-memory queue that pushes
/// into that store. Built by `PreviewData.echoingQueue`.
struct QueuePreviewFixture {
    let store: PlaybackStore
    let queue: Queue
}

/// The in-memory queue behind `PreviewData.echoingQueue`: holds the two
/// lanes, applies each command the way core would, and pushes the result
/// into the store as a fresh snapshot. A dependency fake — the views under
/// shot run their production code against it.
@MainActor
private final class PreviewQueueModel {
    private let store: PlaybackStore
    private var manual: [BridgeQueueEntry]
    private var upcoming: [BridgeQueueEntry]
    /// The context's unshuffled order, kept so turning shuffle off restores
    /// it (filtered to entries still present).
    private let orderedUpcoming: [BridgeQueueEntry]
    private var hasContext: Bool
    private var shuffled: Bool
    /// Uniques the entry ids this model mints for inserted tracks.
    private var mintedEntryCount = 0
    let upcomingList: PreviewQueueList

    init(
        store: PlaybackStore,
        manual: [BridgeQueueEntry],
        upcoming: [BridgeQueueEntry],
        hasContext: Bool,
        shuffled: Bool,
        upcomingList: PreviewQueueList
    ) {
        self.upcomingList = upcomingList
        self.store = store
        self.manual = manual
        self.upcoming = upcoming
        orderedUpcoming = upcoming
        self.hasContext = hasContext
        self.shuffled = shuffled
        apply()
    }

    private func apply() {
        upcomingList.show(hasContext ? upcoming : [])
        store.applyQueueSnapshot(
            BridgeQueueSnapshot(
                manual: manual,
                context: hasContext
                    ? BridgePlaybackContext(
                        kind: .release,
                        sourceTitle: "Neon Frequencies"
                    ) : nil,
                shuffled: shuffled,
                hasNext: !manual.isEmpty || !upcoming.isEmpty,
                hasPrevious: false
            )
        )
    }

    func remove(_ entryId: String) {
        manual.removeAll { $0.entryId == entryId }
        upcoming.removeAll { $0.entryId == entryId }
        apply()
    }

    func clearUpNext() {
        manual = []
        apply()
    }

    /// Drop the context lane: its rows and its section go, the now-playing
    /// track stays — core's `clear_playing_from`.
    func clearPlayingFrom() {
        upcoming = []
        hasContext = false
        apply()
    }

    /// Move `entryId` before `beforeEntryId` within its lane; `nil` moves
    /// it to the lane's end — core's reorder semantics — and answer whether
    /// it moved.
    func reorder(_ entryId: String, before beforeEntryId: String?)
        -> Bool
    {
        func moved(in lane: [BridgeQueueEntry]) -> [BridgeQueueEntry]? {
            guard
                let from = lane.firstIndex(where: {
                    $0.entryId == entryId
                })
            else {
                return nil
            }
            var lane = lane
            let entry = lane.remove(at: from)
            if let beforeEntryId,
                let to = lane.firstIndex(where: {
                    $0.entryId == beforeEntryId
                })
            {
                lane.insert(entry, at: to)
            }
            else {
                lane.append(entry)
            }
            return lane
        }
        if let lane = moved(in: manual) {
            manual = lane
            apply()
            return true
        }
        if let lane = moved(in: upcoming) {
            upcoming = lane
            apply()
            return true
        }
        return false
    }

    /// Play the entry: it becomes now playing, and it — plus whatever sat
    /// before it in its lane — leaves the queue, the way core drains
    /// skipped-over entries.
    func skipTo(_ entryId: String) {
        let entry: BridgeQueueEntry
        if let index = manual.firstIndex(where: { $0.entryId == entryId }) {
            entry = manual[index]
            manual.removeSubrange(...index)
        }
        else if let index = upcoming.firstIndex(where: {
            $0.entryId == entryId
        }) {
            entry = upcoming[index]
            upcoming.removeSubrange(...index)
        }
        else {
            return
        }
        store.play(
            track: BridgeNowPlayingTrack(
                track: BridgePlayingTrack(
                    trackId: entry.trackId,
                    // The queue entry carries only a clock label, not raw ms;
                    // the shot now-playing bar just needs a plausible total.
                    durationMs: 200_000
                ),
                display: entry.display
            )
        )
        apply()
    }

    func setShuffle(_ on: Bool) {
        shuffled = on
        if on {
            upcoming.shuffle()
        }
        else {
            let present = Set(upcoming.map(\.entryId))
            upcoming = orderedUpcoming.filter {
                present.contains($0.entryId)
            }
        }
        apply()
    }

    func insert(trackIds: [String], at index: Int) {
        // A cross-lane drag inserts a context row's track: carry its
        // metadata over. Unknown ids (a library drop) get a bare entry.
        let entries = trackIds.map { trackId in
            mintedEntryCount += 1
            let source =
                upcoming.first { $0.trackId == trackId }
                ?? PreviewData.queueEntries.first {
                    $0.trackId == trackId
                }
            return BridgeQueueEntry(
                entryId: "preview-minted-\(mintedEntryCount)",
                trackId: trackId,
                display: source?.display
                    ?? BridgeTrackDisplay(
                        title: "Track \(trackId)",
                        artistLine: .named("Artist Name"),
                        albumId: "a-01",
                        releaseId: "r-01",
                        albumTitle: "Album Title",
                        coverImage: nil
                    ),
                durationClock: source?.durationClock
                    ?? bridgeLengthClock(ms: 200_000)
            )
        }
        let at = min(max(index, 0), manual.count)
        manual.insert(contentsOf: entries, at: at)
        apply()
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
    /// manual lane, and a context of `context`'s kind. Used by shots that
    /// show the store's queue state but not the context's rows, which only
    /// `echoingQueue`'s upcoming read serves.
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
                hasPrevious: false
            )
        )
        return store
    }

    /// A store plus a `Queue` whose commands mutate an in-memory queue and
    /// re-apply a fresh snapshot, and whose upcoming read answers out of it
    /// — the shot stand-in for core's queue subscriptions. Echo-driven view behavior (the
    /// post-reorder hold clearing, the removal unmount, the shuffle flip)
    /// only works in a shot against this; against `Queue.stub()` the echo
    /// never lands and a second drag starts from a stale display order.
    @MainActor
    static func echoingQueue(
        manualCount: Int,
        context: Bool = true,
        shuffled: Bool = false,
        upcomingRequestFails: Bool = false
    ) -> QueuePreviewFixture {
        let store = PlaybackStore(trackPositions: queuePositions())
        let upcomingList = PreviewQueueList(
            failure: upcomingRequestFails
                ? BridgeError.Diagnostic(
                    category: .database,
                    detail: "database is locked"
                ) : nil
        )
        let model = PreviewQueueModel(
            store: store,
            manual: Array(queueEntries.prefix(manualCount)),
            upcoming: context
                ? Array(queueEntries.suffix(from: manualCount)) : [],
            hasContext: context,
            shuffled: shuffled,
            upcomingList: upcomingList
        )
        return QueuePreviewFixture(
            store: store,
            queue: Queue(
                insertInQueue: { ids, index in
                    Task { @MainActor in
                        model.insert(trackIds: ids, at: Int(index))
                    }
                },
                removeEntry: { id in
                    await model.remove(id)
                },
                clearUpNext: {
                    await model.clearUpNext()
                },
                clearPlayingFrom: {
                    await model.clearPlayingFrom()
                },
                reorderEntry: { id, before in
                    await model.reorder(id, before: before)
                },
                skipToEntry: { id in
                    await model.skipTo(id)
                },
                setShuffle: { on in
                    Task { @MainActor in
                        model.setShuffle(on)
                    }
                },
                upcomingQueries: { _ in upcomingList.queries }
            )
        )
    }

    // MARK: - Now Playing

    static let nowPlayingTitle = "Broadcast"
    static let nowPlayingArtist = "The Midnight Signal"

    // MARK: - Search

    /// Search results for the SearchView "With results" shot.
    static let searchResults = SearchResults(
        bridge: BridgeSearchResults(
            albums: [
                BridgeAlbumSearchResult(
                    id: "a-02",
                    title: "Album Title B",
                    year: 2019,
                    artistLine: .named("Artist Name A"),
                    cover: nil
                ),
                BridgeAlbumSearchResult(
                    id: "a-14",
                    title: "Album Title N",
                    year: 2022,
                    artistLine: .named("Artist Name A"),
                    cover: nil
                ),
                BridgeAlbumSearchResult(
                    id: "a-03",
                    title: "Album Title C",
                    year: 2021,
                    artistLine: .named("Artist Name B"),
                    cover: nil
                ),
            ],
            artists: [
                BridgeArtistSummary(
                    artistId: "artist-a",
                    name: "Artist Name A",
                    countLine: BridgeCountLabel(
                        key: "core.library.album_count",
                        count: 2
                    ),
                    image: nil
                )
            ],
            tracks: [
                BridgeTrackSearchResult(
                    id: "t-03",
                    title: "Track Title 3",
                    durationClock: bridgeLengthClock(ms: 198_000),
                    releaseId: "r-02",
                    albumId: "a-02",
                    albumTitle: "Album Title B",
                    artistLine: .named("Artist Name A"),
                    cover: nil
                ),
                BridgeTrackSearchResult(
                    id: "t-05",
                    title: "Track Title 5",
                    durationClock: bridgeLengthClock(ms: 187_000),
                    releaseId: "r-03",
                    albumId: "a-03",
                    albumTitle: "Album Title C",
                    artistLine: .named("Artist Name B"),
                    cover: nil
                ),
            ],
            composers: [
                BridgeComposerSummary(
                    artistId: "artist-composer-a",
                    name: "Composer Name A",
                    sortName: "Composer Name A",
                    countLine: BridgeCountLabel(
                        key: "core.library.work_count",
                        count: 2
                    ),
                    linkedReleaseCount: 3,
                    unlinkedCreditCount: 0,
                    image: nil
                )
            ],
            works: [
                BridgeWorkSummary(
                    workId: "work-a",
                    title: "Work Title A",
                    disambiguation: nil,
                    workType: "work",
                    composerNames: "Composer Name A",
                    linkedReleaseCount: 2,
                    representativeReleaseId: "release-a",
                    representativeCover: nil
                )
            ],
            topPlayable: nil,
            found: true
        ),
        query: "placeholder"
    )
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
