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
            artistNames: "Artist Name",
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
            artistNames: "Artist Name",
            albumId: "album-1",
            releaseId: "release-1",
            albumTitle: "Album Title",
            coverImage: nil
        ),
        durationClock: bridgeLengthClock(ms: 180_000)
    )
}

/// An upcoming read the test drives: it records every window request and
/// hands `next` whatever the test delivers.
private final class UpcomingFeed: @unchecked Sendable {
    private let lock = NSLock()
    private var opened = 0
    private var requests: [[BridgeLibraryPageWindow]] = []
    private var pending: [BridgeQueueUpcomingSnapshot] = []
    private var askedCount = 0
    private var waiter:
        CheckedContinuation<BridgeQueueUpcomingSnapshot, any Error>?

    var openedCount: Int { lock.withLock { opened } }
    var requested: [[BridgeLibraryPageWindow]] { lock.withLock { requests } }
    /// How many times the store has asked for a value. It applies each value
    /// before asking again, so an ask past a delivered value means that value
    /// was handled.
    var asks: Int { lock.withLock { askedCount } }

    func query() -> QueueUpcomingQuery {
        lock.withLock { opened += 1 }
        return QueueUpcomingQuery(
            setWindows: { [self] windows in
                lock.withLock { requests.append(windows) }
            },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: BridgeQueueUpcomingSnapshot? = lock.withLock {
                        askedCount += 1
                        if pending.isEmpty {
                            waiter = continuation
                            return nil
                        }
                        return pending.removeFirst()
                    }
                    if let ready { continuation.resume(returning: ready) }
                }
            },
            cancel: {}
        )
    }

    /// End the waiting read with `error`.
    func fail(_ error: any Error) {
        let waiter:
            CheckedContinuation<BridgeQueueUpcomingSnapshot, any Error>? =
                lock.withLock {
                    let waiter = self.waiter
                    self.waiter = nil
                    return waiter
                }
        waiter?.resume(throwing: error)
    }

    /// Answer with a tail of `total` entries, one of them, `entryId`, read at
    /// `offset` when given.
    func deliver(
        total: UInt64,
        queueChanged: Bool,
        offset: UInt64? = nil,
        entryId: String = ""
    ) {
        let snapshot = BridgeQueueUpcomingSnapshot(
            upcomingTotal: total,
            queueChanged: queueChanged,
            windows: offset.map { offset in
                [
                    BridgeQueueUpcomingWindow(
                        window: BridgeLibraryPageWindow(
                            offset: offset,
                            limit: 1
                        ),
                        entries: [queueEntry(entryId)]
                    )
                ]
            } ?? []
        )
        let waiter:
            CheckedContinuation<BridgeQueueUpcomingSnapshot, any Error>? =
                lock.withLock {
                    if let waiter = self.waiter {
                        self.waiter = nil
                        return waiter
                    }
                    pending.append(snapshot)
                    return nil
                }
        waiter?.resume(returning: snapshot)
    }
}

private func contextSnapshot(manual: [String] = []) -> BridgeQueueSnapshot {
    BridgeQueueSnapshot(
        manual: manual.map(queueEntry),
        context: BridgePlaybackContext(
            kind: .library,
            sourceTitle: nil,
            shuffled: false
        ),
        hasNext: true,
        hasPrevious: false
    )
}

/// A store showing a context whose upcoming read has answered with a tail of
/// `total` entries.
@MainActor
private func storeReading(
    total: UInt64,
    feed: UpcomingFeed,
    queue: Queue
) async throws -> PlaybackStore {
    let store = PlaybackStore()
    store.applyQueueSnapshot(contextSnapshot())
    store.showUpcoming(queue: queue)
    feed.deliver(total: total, queueChanged: false)
    try await Wait.until { store.upcomingTotal == Int(total) }
    return store
}

@Suite("PlaybackStore upcoming windows")
struct PlaybackStoreUpcomingWindowTests {
    @MainActor
    @Test("scrolling the queue moves one upcoming read's windows")
    func scrollingMovesOneRead() async throws {
        let feed = UpcomingFeed()
        let queue = Queue(subscribeUpcoming: { feed.query() })
        let store = try await storeReading(total: 500, feed: feed, queue: queue)

        for offset in [0, 100, 200, 300] {
            await store.loadUpcomingRange(
                offset: offset,
                limit: 60,
                queue: queue
            )
        }

        #expect(feed.openedCount == 1)
        #expect(
            feed.requested.last == [
                BridgeLibraryPageWindow(offset: 100, limit: 60),
                BridgeLibraryPageWindow(offset: 200, limit: 60),
                BridgeLibraryPageWindow(offset: 300, limit: 60),
            ],
            "the window farthest from the newest one is dropped"
        )

        await store.loadUpcomingRange(offset: 210, limit: 20, queue: queue)
        #expect(
            feed.requested.count == 4,
            "a range a read window covers requests nothing"
        )
    }

    @MainActor
    @Test(
        "the lane's length and rows come from one read, and only a change to the queue counts as one"
    )
    func lengthAndRowsComeTogether() async throws {
        let feed = UpcomingFeed()
        let queue = Queue(subscribeUpcoming: { feed.query() })
        let store = try await storeReading(total: 500, feed: feed, queue: queue)
        let changes = store.contextEpoch
        await store.loadUpcomingRange(offset: 100, limit: 60, queue: queue)

        feed.deliver(
            total: 500,
            queueChanged: false,
            offset: 100,
            entryId: "first"
        )
        try await Wait.until { store.upcomingItem(at: 100) != nil }
        #expect(store.upcomingItem(at: 100)?.entryId == "first")
        #expect(store.contextEpoch == changes, "a scroll is not a change")

        feed.deliver(
            total: 300,
            queueChanged: true,
            offset: 100,
            entryId: "second"
        )
        try await Wait.until {
            store.upcomingItem(at: 100)?.entryId == "second"
        }
        #expect(store.upcomingTotal == 300)
        #expect(store.contextEpoch == changes + 1)
        #expect(feed.openedCount == 1)
    }

    @MainActor
    @Test("a queue with no context ends the read and empties the lane")
    func noContextEndsTheRead() async throws {
        let feed = UpcomingFeed()
        let queue = Queue(subscribeUpcoming: { feed.query() })
        let store = try await storeReading(total: 5, feed: feed, queue: queue)

        store.applyQueueSnapshot(
            BridgeQueueSnapshot(
                manual: [],
                context: nil,
                hasNext: false,
                hasPrevious: false
            )
        )

        #expect(store.upcomingTotal == 0)
        #expect(store.upcomingItem(at: 0) == nil)
        store.applyQueueSnapshot(contextSnapshot())
        store.showUpcoming(queue: queue)
        #expect(feed.openedCount == 2, "a new context opens the read again")
    }

    @MainActor
    @Test("every queue value counts as a change to the manual lane")
    func queueValuesCountManualChanges() {
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot(manual: ["a"]))
        let changes = store.manualEpoch
        store.applyQueueSnapshot(contextSnapshot(manual: ["a"]))
        #expect(store.manualEpoch == changes + 1)
        #expect(store.manualQueue.map(\.entryId) == ["a"])
    }
}

@Suite("PlaybackStore upcoming read failure")
struct PlaybackStoreUpcomingFailureTests {
    @MainActor
    @Test("a failed read says why, and Retry opens it again over its windows")
    func failedReadRetries() async throws {
        let feed = UpcomingFeed()
        let queue = Queue(subscribeUpcoming: { feed.query() })
        let store = try await storeReading(total: 500, feed: feed, queue: queue)
        await store.loadUpcomingRange(offset: 100, limit: 60, queue: queue)
        try await Wait.until { feed.asks >= 2 }

        feed.fail(
            BridgeError.Diagnostic(
                category: .database,
                detail: "database is locked"
            )
        )
        try await Wait.until { store.upcomingFailure != nil }
        #expect(store.upcomingFailure?.detail == "database is locked")

        await store.retryUpcoming(queue: queue)
        #expect(store.upcomingFailure == nil)
        #expect(feed.openedCount == 2, "Retry opens the read again")
        #expect(
            feed.requested.last == [
                BridgeLibraryPageWindow(offset: 100, limit: 60)
            ],
            "over the windows it covered"
        )
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

        store.beginLoading(trackId: "b")

        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.isActive)
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    @MainActor
    @Test("retains the paused track until the target metadata arrives")
    func retainsPausedTrack() {
        let store = PlaybackStore()
        store.pause(track: makeTrack("a"), reason: .manual)

        store.beginLoading(trackId: "b")

        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    @MainActor
    @Test("switches to the target once its metadata lands")
    func switchesToTarget() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.beginLoading(trackId: "b")
        #expect(store.nowPlaying.track?.track.trackId == "a")

        store.setLoadingTarget(makeTrack("b"))
        #expect(store.nowPlaying.track?.track.trackId == "b")
        #expect(store.nowPlaying.loadingTrackId == "b")
        #expect(store.nowPlaying.isActive)
    }

    @MainActor
    @Test("ignores a target for a stale track id")
    func ignoresStaleTarget() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))
        store.beginLoading(trackId: "b")

        store.setLoadingTarget(makeTrack("stale"))

        #expect(store.nowPlaying.loadingTrackId == "b")
        #expect(store.nowPlaying.track?.track.trackId == "a")
    }

    @MainActor
    @Test("cold-start loading has no displayed track but is active")
    func coldStartHasNoTrack() {
        let store = PlaybackStore()

        store.beginLoading(trackId: "x")

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

        store.setLoadingTarget(makeTrack("a"))

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.track.trackId == "a")
        #expect(store.nowPlaying.isActive)
    }

    @MainActor
    @Test("a seek while paused re-enters loading for the current track")
    func seekWhilePausedEntersLoading() {
        let store = PlaybackStore()
        store.pause(track: makeTrack("a"), reason: .manual)

        store.setLoadingTarget(makeTrack("a"))

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.track.trackId == "a")
    }

    /// Without a bare loading state first, a target for another track is stale.
    @MainActor
    @Test("ignores a resolved target for a track other than the one playing")
    func ignoresTargetForOtherPlayingTrack() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.setLoadingTarget(makeTrack("b"))

        #expect(store.nowPlaying.loadingTrackId == nil)
        #expect(store.nowPlaying.track?.track.trackId == "a")
    }
}

@Suite("NowPlaying state")
struct NowPlayingStateTests {
    @Test("loading counts as playing so the transport shows the pause glyph")
    func loadingIsPlaying() {
        #expect(
            NowPlaying.loading(.unprepared(trackId: "x"), previous: nil)
                .isPlaying
        )
    }

    @Test("loadingTrackId is nil when not loading")
    func loadingTrackIdNilWhenNotLoading() {
        #expect(NowPlaying.playing(makeTrack("a")).loadingTrackId == nil)
        #expect(
            NowPlaying.paused(makeTrack("a"), reason: .manual).loadingTrackId
                == nil
        )
        #expect(NowPlaying.stopped.loadingTrackId == nil)
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

@Suite("PlaybackStore seek projection")
struct PlaybackStoreSeekProjectionTests {
    @MainActor
    @Test("forward seek publishes the dropped target and ignores old progress")
    func forwardSeekPublishesTarget() {
        let store = PlaybackStore()
        _ = store.updatePlaybackPosition(
            positionMs: 10_000,
            durationMs: 100_000,
            progress: 0.1
        )

        let projected = store.projectSeek(ratio: 0.75)

        #expect(projected?.positionMs == 75_000)
        expectStorePosition(
            store,
            progress: 0.75,
            positionMs: 75_000,
            durationMs: 100_000
        )
        let stale = store.updatePlaybackPosition(
            positionMs: 20_000,
            durationMs: 100_000,
            progress: 0.2
        )
        #expect(stale.positionMs == 75_000)

        let nearTargetProgress = store.updatePlaybackPosition(
            positionMs: 75_050,
            durationMs: 100_000,
            progress: 0.7505
        )

        #expect(nearTargetProgress.positionMs == 75_000)

        let stillProjected = store.updatePlaybackPosition(
            positionMs: 80_000,
            durationMs: 100_000,
            progress: 0.8
        )

        #expect(stillProjected.positionMs == 75_000)
        expectStorePosition(
            store,
            progress: 0.75,
            positionMs: 75_000,
            durationMs: 100_000
        )

        let accepted = store.updatePlaybackSeeked(
            positionMs: 76_000,
            durationMs: 100_000,
            progress: 0.76
        )

        #expect(accepted.positionMs == 76_000)
        expectStorePosition(
            store,
            progress: 0.76,
            positionMs: 76_000,
            durationMs: 100_000
        )
    }

    @MainActor
    @Test("forward seek rejects stale progress before the target")
    func forwardSeekRejectsOriginSideProgressNearTarget() {
        let store = PlaybackStore()
        _ = store.updatePlaybackPosition(
            positionMs: 24_500,
            durationMs: 100_000,
            progress: 0.245
        )

        _ = store.projectSeek(ratio: 0.25)

        let stale = store.updatePlaybackPosition(
            positionMs: 24_600,
            durationMs: 100_000,
            progress: 0.246
        )
        #expect(stale.positionMs == 25_000)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.25,
            positionMs: 25_000,
            durationMs: 100_000
        )

        let accepted = store.updatePlaybackPosition(
            positionMs: 25_000,
            durationMs: 100_000,
            progress: 0.25
        )

        #expect(accepted.positionMs == 25_000)
    }

    @MainActor
    @Test("backward seek holds the target until progress reaches it")
    func backwardSeekPublishesTarget() {
        let store = PlaybackStore()
        _ = store.updatePlaybackPosition(
            positionMs: 80_000,
            durationMs: 100_000,
            progress: 0.8
        )

        let projected = store.projectSeek(ratio: 0.25)

        #expect(projected?.positionMs == 25_000)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.25,
            positionMs: 25_000,
            durationMs: 100_000
        )
        let stale = store.updatePlaybackPosition(
            positionMs: 70_000,
            durationMs: 100_000,
            progress: 0.7
        )
        #expect(stale.positionMs == 25_000)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.25,
            positionMs: 25_000,
            durationMs: 100_000
        )

        let reachedTargetProgress = store.updatePlaybackPosition(
            positionMs: 25_100,
            durationMs: 100_000,
            progress: 0.251
        )

        #expect(reachedTargetProgress.positionMs == 25_000)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.25,
            positionMs: 25_000,
            durationMs: 100_000
        )

        let accepted = store.updatePlaybackSeeked(
            positionMs: 25_100,
            durationMs: 100_000,
            progress: 0.251
        )

        #expect(accepted.positionMs == 25_100)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.251,
            positionMs: 25_100,
            durationMs: 100_000
        )
    }

    @MainActor
    @Test("backward seek rejects stale progress near the origin")
    func backwardSeekRejectsOriginSideProgressNearTarget() {
        let store = PlaybackStore()
        _ = store.updatePlaybackPosition(
            positionMs: 25_900,
            durationMs: 100_000,
            progress: 0.259
        )

        _ = store.projectSeek(ratio: 0.25)

        let stale = store.updatePlaybackPosition(
            positionMs: 25_950,
            durationMs: 100_000,
            progress: 0.2595
        )
        #expect(stale.positionMs == 25_000)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.25,
            positionMs: 25_000,
            durationMs: 100_000
        )

        let reachedTargetProgress = store.updatePlaybackPosition(
            positionMs: 25_100,
            durationMs: 100_000,
            progress: 0.251
        )

        #expect(reachedTargetProgress.positionMs == 25_000)

        let accepted = store.updatePlaybackSeeked(
            positionMs: 25_100,
            durationMs: 100_000,
            progress: 0.251
        )

        #expect(accepted.positionMs == 25_100)
    }

    @MainActor
    @Test("unknown duration seek leaves the position unchanged")
    func unknownDurationSeekDoesNotProject() {
        let store = PlaybackStore()
        _ = store.updatePlaybackPosition(
            positionMs: 10_000,
            durationMs: nil,
            progress: 0.1
        )

        #expect(store.projectSeek(ratio: 0.75) == nil)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.1,
            positionMs: 10_000,
            durationMs: nil
        )
    }

    @MainActor
    @Test("pregap position remains negative in the published event")
    func pregapPositionRemainsNegative() {
        let store = PlaybackStore()

        _ = store.updatePlaybackPosition(
            positionMs: -1_250,
            durationMs: 100_000,
            progress: 0
        )

        expectPosition(
            store.playbackPositionEvent,
            progress: 0,
            positionMs: -1_250,
            durationMs: 100_000
        )
    }

    @MainActor
    @Test("reset clears projected position")
    func resetClearsProjection() {
        let store = PlaybackStore()
        _ = store.updatePlaybackPosition(
            positionMs: 10_000,
            durationMs: 100_000,
            progress: 0.1
        )
        _ = store.projectSeek(ratio: 0.75)

        store.resetPlaybackPosition()

        guard case .reset = store.playbackPositionEvent else {
            Issue.record("position did not reset")
            return
        }
    }

    @MainActor
    @Test("same-track loading keeps the projected seek position")
    func sameTrackLoadingKeepsProjectedPosition() {
        let store = PlaybackStore()
        store.play(track: makeTrack("track-1"))
        _ = store.updatePlaybackPosition(
            positionMs: 10_000,
            durationMs: 100_000,
            progress: 0.1
        )

        _ = store.projectSeek(ratio: 0.75)
        store.beginLoading(trackId: "track-1")

        expectPosition(
            store.playbackPositionEvent,
            progress: 0.75,
            positionMs: 75_000,
            durationMs: 100_000
        )
    }
}

private func expectPosition(
    _ event: PlaybackPositionEvent,
    progress: Double,
    positionMs: Int64,
    durationMs: UInt64?
) {
    guard
        case .position(
            let actualProgress,
            let actualPositionMs,
            let actualDurationMs
        ) = event
    else {
        Issue.record("position event was reset")
        return
    }
    #expect(actualProgress == progress)
    #expect(actualPositionMs == positionMs)
    #expect(actualDurationMs == durationMs)
}

@MainActor
private func expectStorePosition(
    _ store: PlaybackStore,
    progress: Double,
    positionMs: Int64,
    durationMs: UInt64?
) {
    expectPosition(
        store.playbackPositionEvent,
        progress: progress,
        positionMs: positionMs,
        durationMs: durationMs
    )
}

extension PlaybackStore {
    fileprivate func updatePlaybackPosition(
        positionMs: Int64,
        durationMs: UInt64?,
        progress: Double
    ) -> PlaybackPositionSnapshot {
        guard
            let snapshot = updatePlaybackProgress(
                trackId: "track-1",
                positionMs: positionMs,
                durationMs: durationMs,
                progress: progress
            )
        else {
            preconditionFailure("playback progress targeted the test track")
        }
        return snapshot
    }

    fileprivate func updatePlaybackSeeked(
        positionMs: Int64,
        durationMs: UInt64?,
        progress: Double
    ) -> PlaybackPositionSnapshot {
        guard
            let snapshot = updatePlaybackSeeked(
                trackId: "track-1",
                positionMs: positionMs,
                durationMs: durationMs,
                progress: progress
            )
        else {
            preconditionFailure("playback seeked targeted the test track")
        }
        return snapshot
    }
}
