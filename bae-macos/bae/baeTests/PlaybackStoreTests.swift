import BaeKit
import Foundation
import Testing

@testable import bae

private func makeTrack(_ id: String) -> BridgeNowPlayingTrack {
    BridgeNowPlayingTrack(
        track: BridgePlayingTrack(
            trackId: id,
            durationMs: nil
        ),
        display: .library(
            display: BridgeTrackDisplay(
                title: "Title \(id)",
                artistLine: .named("Artist Name"),
                albumId: "album-1",
                releaseId: "release-1",
                albumTitle: "Album Title",
                coverImage: nil
            )
        )
    )
}

private func queueEntry(_ entryId: String) -> BridgeQueueEntry {
    BridgeQueueEntry(
        entryId: entryId,
        trackId: "track-\(entryId)",
        display: BridgeTrackDisplay(
            title: "Track Title",
            artistLine: .named("Artist Name"),
            albumId: "album-1",
            releaseId: "release-1",
            albumTitle: "Album Title",
            coverImage: nil
        ),
        durationClock: BridgeDurationClock.length(ms: 180_000)
    )
}

private func contextSnapshot(
    manual: [String] = [],
    shuffled: Bool = false
) -> BridgeQueueSnapshot {
    BridgeQueueSnapshot(
        manual: manual.map(queueEntry),
        context: BridgePlaybackContext(
            kind: .library,
            sourceTitle: nil
        ),
        shuffled: shuffled,
        hasNext: true,
        hasPrevious: false,
        upNextCount: UInt32(manual.count),
        isEmpty: false
    )
}

/// Records each upcoming list the queue opens and whether the store asked
/// for it shuffled; a test shows rows in the newest list.
private final class OpenedLists: @unchecked Sendable {
    private let lock = NSLock()
    private var orders: [Bool] = []
    private var lists: [PreviewQueueList] = []

    /// A queue whose upcoming queries are `makeList`'s lists.
    func queue(
        makeList: @escaping @Sendable (_ opened: Int) -> PreviewQueueList = {
            _ in PreviewQueueList()
        }
    ) -> Queue {
        Queue(upcomingQueries: { [self] shuffled in
            let list = lock.withLock {
                orders.append(shuffled)
                let list = makeList(orders.count)
                lists.append(list)
                return list
            }
            Wait.happened()
            return list.queries
        })
    }

    var openedOrders: [Bool] { lock.withLock { orders } }
    var latest: PreviewQueueList { lock.withLock { lists[lists.count - 1] } }
}

@Suite("PlaybackStore upcoming list")
struct PlaybackStoreUpcomingListTests {
    @MainActor
    @Test("the lane's length and rows are the list's count and windows")
    func lengthAndRowsAreTheLists() async throws {
        let opened = OpenedLists()
        let queue = opened.queue()
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot())
        let list = Task { @MainActor in await store.showUpcoming(queue: queue) }
        try await Wait.until { opened.openedOrders.count == 1 }
        opened.latest.show((0..<300).map { queueEntry("e\($0)") })
        await list.value

        try await Wait.until { store.upcomingTotal == 300 }
        let hold = Task { await store.holdUpcoming(at: 0) }
        try await Wait.until { store.upcomingItem(at: 0)?.entryId == "e0" }

        opened.latest.show((0..<250).map { queueEntry("f\($0)") })
        try await Wait.until {
            store.upcomingItem(at: 0)?.entryId == "f0"
                && store.upcomingTotal == 250
        }
        #expect(opened.openedOrders == [false], "a change opens no list")
        hold.cancel()
    }

    @MainActor
    @Test(
        "a shuffle flip opens the list in the other order and drops the first"
    )
    func shuffleFlipOpensTheOtherOrder() async throws {
        let opened = OpenedLists()
        let queue = opened.queue()
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot())
        let first = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 1 }
        opened.latest.show((0..<5).map { queueEntry("e\($0)") })
        await first.value
        let before = try #require(store.upcomingEpoch)

        store.applyQueueSnapshot(contextSnapshot(shuffled: true))
        #expect(
            store.upcomingEpoch == before,
            "the first list stays until the new one answers"
        )
        let second = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 2 }
        opened.latest.show((0..<5).map { queueEntry("s\($0)") })
        await second.value

        #expect(opened.openedOrders == [false, true])
        #expect(store.upcomingEpoch != before)
        try await Wait.until { store.upcomingTotal == 5 }
    }

    @MainActor
    @Test("a queue with no context ends the list and empties the lane")
    func noContextEndsTheList() async throws {
        let opened = OpenedLists()
        let queue = opened.queue()
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot())
        let first = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 1 }
        opened.latest.show((0..<5).map { queueEntry("e\($0)") })
        await first.value
        #expect(store.upcomingEpoch != nil)

        store.applyQueueSnapshot(
            BridgeQueueSnapshot(
                manual: [],
                context: nil,
                shuffled: false,
                hasNext: false,
                hasPrevious: false,
                upNextCount: 0,
                isEmpty: true
            )
        )
        #expect(store.upcomingEpoch == nil)

        store.applyQueueSnapshot(contextSnapshot())
        let second = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 2 }
        opened.latest.show([queueEntry("n0")])
        await second.value
        // A new context opens a list again; its count arrives after its rows.
        try await Wait.until { store.upcomingTotal == 1 }
    }

    @MainActor
    @Test("shuffle is core's mode, shown with no context too")
    func shuffleIsCoresMode() {
        let store = PlaybackStore()
        store.applyQueueSnapshot(
            BridgeQueueSnapshot(
                manual: [],
                context: nil,
                shuffled: true,
                hasNext: false,
                hasPrevious: false,
                upNextCount: 0,
                isEmpty: true
            )
        )
        #expect(store.queueContext == nil)
        #expect(store.shuffled)
    }

    @MainActor
    @Test("Up Next is the queue value's rows")
    func upNextIsTheValuesRows() {
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot(manual: ["a"]))
        #expect(store.manualQueue.map(\.entryId) == ["a"])
        store.applyQueueSnapshot(contextSnapshot(manual: ["a", "b"]))
        #expect(store.manualQueue.map(\.entryId) == ["a", "b"])
    }
}

@Suite("PlaybackStore upcoming read failure")
struct PlaybackStoreUpcomingFailureTests {
    @MainActor
    @Test("a failed read says why, and Retry opens the list again")
    func failedReadRetries() async throws {
        let opened = OpenedLists()
        let queue = opened.queue(makeList: { opened in
            opened == 1
                ? PreviewQueueList(
                    failure: BridgeError.Diagnostic(
                        category: .database,
                        detail: "database is locked"
                    )
                ) : PreviewQueueList()
        })
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot())
        await store.showUpcoming(queue: queue)
        #expect(store.upcomingFailure?.detail == "database is locked")

        let retry = Task { @MainActor in await store.retryUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 2 }
        opened.latest.show((0..<3).map { queueEntry("e\($0)") })
        await retry.value
        #expect(store.upcomingFailure == nil)
        try await Wait.until { store.upcomingTotal == 3 }
    }

    @MainActor
    @Test("the list a shuffle flip opens drops the old list's read failure")
    func shuffleFlipDropsTheOldFailure() async throws {
        let opened = OpenedLists()
        let queue = opened.queue()
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot())
        let first = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 1 }
        opened.latest.show((0..<5).map { queueEntry("e\($0)") })
        await first.value
        opened.latest.failWindows(
            BridgeError.Diagnostic(
                category: .database,
                detail: "database is locked"
            )
        )
        try await Wait.until {
            store.upcomingFailure?.detail == "database is locked"
        }

        store.applyQueueSnapshot(contextSnapshot(shuffled: true))
        let second = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 2 }
        opened.latest.show((0..<5).map { queueEntry("s\($0)") })
        await second.value
        #expect(store.upcomingFailure == nil)
    }
}

@Suite("PlaybackStore now playing")
struct PlaybackStoreNowPlayingTests {
    /// A load names its track with the library's display of it from its
    /// first value, so the bar shows the next track at once.
    @MainActor
    @Test("a load shows the next track before core prepares it")
    func aLoadShowsTheNextTrack() {
        let store = PlaybackStore()
        store.apply(.playing(track: makeTrack("a")))

        store.apply(
            .loading(
                track: .unprepared(
                    trackId: "b",
                    display: makeTrack("b").display
                ),
                waitsOnStorage: false
            )
        )

        #expect(store.nowPlaying.trackId == "b")
        #expect(store.nowPlaying.display?.title == "Title b")
        #expect(store.nowPlaying.isActive)
        #expect(store.nowPlaying.isPlaying)
    }

    /// A track the library no longer holds keeps its place and transport,
    /// and shows as removed: no artist, no cover, no album.
    @MainActor
    @Test("a deleted playing track shows as removed")
    func aDeletedTrackShowsAsRemoved() {
        let store = PlaybackStore()
        store.apply(.playing(track: makeTrack("a")))

        store.apply(
            .playing(
                track: BridgeNowPlayingTrack(
                    track: BridgePlayingTrack(trackId: "a", durationMs: nil),
                    display: .removed
                )
            )
        )

        #expect(store.nowPlaying.trackId == "a")
        #expect(store.nowPlaying.isPlaying)
        #expect(store.nowPlaying.display?.library == nil)
        #expect(
            store.nowPlaying.display?.title
                == localizedCoreString("core.playback.removed_track")
        )
        #expect(store.nowPlaying.secondaryLine == nil)
    }

    @Test("only a load that waits on storage shows the wait")
    func onlyALoadThatWaitsShowsTheWait() {
        let track = makeTrack("a")
        #expect(
            BridgePlaybackValueState.loading(
                track: .prepared(track: track),
                waitsOnStorage: true
            )
            .waitsOnStorage
        )
        #expect(
            !BridgePlaybackValueState.loading(
                track: .prepared(track: track),
                waitsOnStorage: false
            )
            .waitsOnStorage
        )
        #expect(!BridgePlaybackValueState.playing(track: track).waitsOnStorage)
        #expect(
            !BridgePlaybackValueState.paused(track: track, reason: .manual)
                .waitsOnStorage
        )
        #expect(!BridgePlaybackValueState.stopped.waitsOnStorage)
    }

    @Test("stopped names no track and is inactive")
    func stoppedIsInactive() {
        let stopped = BridgePlaybackValueState.stopped
        #expect(stopped.trackId == nil)
        #expect(stopped.display == nil)
        #expect(!stopped.isActive)
        #expect(!stopped.isPlaying)
    }
}

@Suite("PlaybackStore side-pause prompt presentation")
struct PlaybackStoreSidePausePromptTests {
    private static let prompt = BridgeSidePausePrompt(
        id: "side-pause-1",
        ended: .sided(sideLetter: "A"),
        countdown: nil
    )

    @MainActor
    @Test("a dismissed prompt stays dismissed through repeated state delivery")
    func repeatedStateDoesNotPresentAgain() {
        let store = PlaybackStore()
        let track = makeTrack("track-1")

        store.apply(
            .paused(track: track, reason: .sideEnded(prompt: Self.prompt))
        )
        #expect(store.presentedSidePausePrompt == Self.prompt)

        store.dismissSidePausePrompt(Self.prompt)
        store.apply(
            .paused(track: track, reason: .sideEnded(prompt: Self.prompt))
        )

        #expect(store.presentedSidePausePrompt == nil)
    }

    @MainActor
    @Test("the same boundary can present after playback leaves the pause")
    func laterPausePresentsAgain() {
        let store = PlaybackStore()
        let track = makeTrack("track-1")

        store.apply(
            .paused(track: track, reason: .sideEnded(prompt: Self.prompt))
        )
        store.dismissSidePausePrompt(Self.prompt)
        store.apply(.playing(track: track))
        store.apply(
            .paused(track: track, reason: .sideEnded(prompt: Self.prompt))
        )

        #expect(store.presentedSidePausePrompt == Self.prompt)
    }

    @MainActor
    @Test("a pause at another boundary presents its prompt after a dismissal")
    func anotherBoundaryPresents() {
        let store = PlaybackStore()
        let track = makeTrack("track-1")
        let next = BridgeSidePausePrompt(
            id: "side-pause-2",
            ended: .sided(sideLetter: "B"),
            countdown: nil
        )

        store.apply(
            .paused(track: track, reason: .sideEnded(prompt: Self.prompt))
        )
        store.dismissSidePausePrompt(Self.prompt)
        store.apply(.paused(track: track, reason: .sideEnded(prompt: next)))

        #expect(store.presentedSidePausePrompt == next)
    }
}
