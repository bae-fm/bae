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
        display: BridgeTrackDisplay(
            title: "Title \(id)",
            artistLine: .named("Artist Name"),
            albumId: "album-1",
            releaseId: "release-1",
            albumTitle: "Album Title",
            coverImage: nil
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
        durationClock: bridgeLengthClock(ms: 180_000)
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
        hasPrevious: false
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
        #expect(store.upcomingTotal == 5)
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
                hasPrevious: false
            )
        )
        #expect(store.upcomingEpoch == nil)

        store.applyQueueSnapshot(contextSnapshot())
        let second = Task { @MainActor in await store.showUpcoming(queue: queue)
        }
        try await Wait.until { opened.openedOrders.count == 2 }
        opened.latest.show([queueEntry("n0")])
        await second.value
        #expect(
            store.upcomingTotal == 1,
            "a new context opens a list again"
        )
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
                hasPrevious: false
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
        #expect(store.upcomingTotal == 3)
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

@Suite("PlaybackStore loading transition")
struct PlaybackStoreBeginLoadingTests {
    /// If `nowPlaying.track` went nil while loading, the now-playing bar would
    /// close on every track change.
    @MainActor
    @Test("retains the playing track until the target metadata arrives")
    func retainsPlayingTrack() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.beginLoading(trackId: "b", waitsOnStorage: false)

        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.isActive)
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    @MainActor
    @Test("retains the paused track until the target metadata arrives")
    func retainsPausedTrack() {
        let store = PlaybackStore()
        store.pause(track: makeTrack("a"), reason: .manual)

        store.beginLoading(trackId: "b", waitsOnStorage: false)

        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    @MainActor
    @Test("switches to the target once its metadata lands")
    func switchesToTarget() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.beginLoading(trackId: "b", waitsOnStorage: false)
        #expect(store.nowPlaying.track?.track.trackId == "a")

        store.setLoadingTarget(makeTrack("b"), waitsOnStorage: false)
        #expect(store.nowPlaying.track?.track.trackId == "b")
        #expect(store.nowPlaying.loadingTrackId == "b")
        #expect(store.nowPlaying.isActive)
    }

    @MainActor
    @Test("ignores a target for a stale track id")
    func ignoresStaleTarget() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))
        store.beginLoading(trackId: "b", waitsOnStorage: false)

        store.setLoadingTarget(makeTrack("stale"), waitsOnStorage: false)

        #expect(store.nowPlaying.loadingTrackId == "b")
        #expect(store.nowPlaying.track?.track.trackId == "a")
    }

    @MainActor
    @Test("cold-start loading has no displayed track but is active")
    func coldStartHasNoTrack() {
        let store = PlaybackStore()

        store.beginLoading(trackId: "x", waitsOnStorage: false)

        #expect(store.nowPlaying.track == nil)
        #expect(store.nowPlaying.loadingTrackId == "x")
        #expect(store.nowPlaying.isActive)
    }

    /// A seek sends only a resolved loading state for the current track, with
    /// no bare one first; the spinner shows and the track stays on screen.
    @MainActor
    @Test("a seek while playing re-enters loading for the current track")
    func seekWhilePlayingEntersLoading() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.setLoadingTarget(makeTrack("a"), waitsOnStorage: false)

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.isActive)
    }

    @MainActor
    @Test("a seek while paused re-enters loading for the current track")
    func seekWhilePausedEntersLoading() {
        let store = PlaybackStore()
        store.pause(track: makeTrack("a"), reason: .manual)

        store.setLoadingTarget(makeTrack("a"), waitsOnStorage: false)

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.track.trackId == "a")
    }

    /// Without a bare loading state first, a target for another track is stale.
    @MainActor
    @Test("ignores a resolved target for a track other than the one playing")
    func ignoresTargetForOtherPlayingTrack() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.setLoadingTarget(makeTrack("b"), waitsOnStorage: false)

        #expect(store.nowPlaying.loadingTrackId == nil)
        #expect(store.nowPlaying.track?.track.trackId == "a")
    }

    /// The bar keeps the track before until the next one's metadata lands, but
    /// what marks or opens the current track must not point at that one.
    @MainActor
    @Test("the current track is the loading one, never the one before it")
    func currentTrackIsTheLoadingOne() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))
        #expect(store.nowPlaying.current?.track.trackId == "a")
        #expect(store.nowPlaying.currentTrackId == "a")

        store.beginLoading(trackId: "b", waitsOnStorage: false)
        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.current == nil)
        #expect(store.nowPlaying.currentTrackId == "b")

        store.setLoadingTarget(makeTrack("b"), waitsOnStorage: false)
        #expect(store.nowPlaying.current?.track.trackId == "b")
        #expect(store.nowPlaying.currentTrackId == "b")

        store.stop()
        #expect(store.nowPlaying.current == nil)
        #expect(store.nowPlaying.currentTrackId == nil)
    }

    /// The arc around Play follows the now-playing value's wait on storage.
    @MainActor
    @Test("the now-playing value waits on storage when the load passes true")
    func waitsOnStorageWhenTheLoadPassesTrue() {
        let store = PlaybackStore()
        store.beginLoading(trackId: "a", waitsOnStorage: false)
        #expect(!store.nowPlaying.waitsOnStorage)
        store.beginLoading(trackId: "a", waitsOnStorage: true)
        #expect(store.nowPlaying.waitsOnStorage)
        store.setLoadingTarget(makeTrack("a"), waitsOnStorage: false)
        #expect(!store.nowPlaying.waitsOnStorage)
        store.setLoadingTarget(makeTrack("a"), waitsOnStorage: true)
        #expect(store.nowPlaying.waitsOnStorage)
        store.play(track: makeTrack("a"))
        #expect(!store.nowPlaying.waitsOnStorage)
    }
}

@Suite("NowPlaying state")
struct NowPlayingStateTests {
    @Test("loading counts as playing so the transport shows the pause glyph")
    func loadingIsPlaying() {
        #expect(
            NowPlaying.loading(
                .unprepared(trackId: "x"),
                previous: nil,
                waitsOnStorage: false
            )
            .isPlaying
        )
    }

    @Test("only a load waits on storage")
    func onlyALoadWaitsOnStorage() {
        #expect(!NowPlaying.playing(makeTrack("a")).waitsOnStorage)
        #expect(
            !NowPlaying.paused(makeTrack("a"), reason: .manual).waitsOnStorage
        )
        #expect(!NowPlaying.stopped.waitsOnStorage)
    }

    @Test("stopped clears the track and is inactive")
    func stoppedIsInactive() {
        let stopped = NowPlaying.stopped
        #expect(stopped.track == nil)
        #expect(!stopped.isActive)
        #expect(!stopped.isPlaying)
    }
}

@Suite("PlaybackStore side-pause prompt presentation")
struct PlaybackStoreSidePausePromptTests {
    private static let prompt = BridgeSidePausePrompt(
        id: "side-pause-1",
        boundary: .side,
        sideLabel: "A",
        countdown: nil
    )

    @MainActor
    @Test("a dismissed prompt stays dismissed through repeated state delivery")
    func repeatedStateDoesNotPresentAgain() {
        let store = PlaybackStore()
        let track = makeTrack("track-1")

        store.pause(track: track, reason: .sideEnded(prompt: Self.prompt))
        #expect(store.presentedSidePausePrompt == Self.prompt)

        store.dismissSidePausePrompt(Self.prompt)
        store.pause(track: track, reason: .sideEnded(prompt: Self.prompt))

        #expect(store.presentedSidePausePrompt == nil)
    }

    @MainActor
    @Test("the same boundary can present after playback leaves the pause")
    func laterPausePresentsAgain() {
        let store = PlaybackStore()
        let track = makeTrack("track-1")

        store.pause(track: track, reason: .sideEnded(prompt: Self.prompt))
        store.dismissSidePausePrompt(Self.prompt)
        store.play(track: track)
        store.pause(track: track, reason: .sideEnded(prompt: Self.prompt))

        #expect(store.presentedSidePausePrompt == Self.prompt)
    }
}

extension NowPlaying {
    /// The loading track's id, which can differ from the displayed track's;
    /// `nil` while nothing loads.
    fileprivate var loadingTrackId: String? {
        guard case .loading(let loading, _, _) = self else { return nil }
        return loading.trackId
    }
}
