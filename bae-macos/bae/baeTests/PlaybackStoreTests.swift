import BaeKit
import Foundation
import Testing

@testable import bae

private func makeTrack(_ id: String) -> NowPlayingTrack {
    NowPlayingTrack(
        trackId: id,
        trackTitle: "Title \(id)",
        artistNames: "Artist Name",
        albumId: "album-1",
        coverImage: nil,
        durationMs: 0
    )
}

private func makeQueueSnapshot(
    entryId: String,
    revision: UInt64
) -> BridgeQueueSnapshot {
    BridgeQueueSnapshot(
        manual: [
            BridgeQueueEntry(
                entryId: entryId,
                trackId: "track-\(entryId)",
                title: "Track Title",
                artistNames: "Artist Name",
                durationClock: bridgeClock(ms: 180_000),
                albumTitle: "Album Title",
                coverImage: nil
            )
        ],
        context: nil,
        hasNext: false,
        hasPrevious: false,
        revision: revision
    )
}

@Suite("PlaybackStore queue revisions")
struct PlaybackStoreQueueRevisionTests {
    @MainActor
    @Test("an older queue read cannot overwrite a newer event")
    func rejectsOlderSnapshot() {
        let store = PlaybackStore()

        store.applyQueueSnapshot(
            makeQueueSnapshot(entryId: "newer", revision: 2)
        )
        store.applyQueueSnapshot(
            makeQueueSnapshot(entryId: "older", revision: 1)
        )

        #expect(store.revision == 2)
        #expect(store.manualQueue.map(\.entryId) == ["newer"])
    }
}

/// An upcoming read the test drives: it records every window request and
/// hands `next` whatever the test delivers.
private final class UpcomingFeed: @unchecked Sendable {
    private let lock = NSLock()
    private var opened = 0
    private var requests: [[BridgeLibraryPageWindow]] = []
    private var pending: [BridgeQueueUpcomingSnapshot] = []
    private var waiter:
        CheckedContinuation<BridgeQueueUpcomingSnapshot, any Error>?

    var openedCount: Int { lock.withLock { opened } }
    var requested: [[BridgeLibraryPageWindow]] { lock.withLock { requests } }

    func query() -> QueueUpcomingQuery {
        lock.withLock { opened += 1 }
        return QueueUpcomingQuery(
            setWindows: { [self] windows in
                lock.withLock { requests.append(windows) }
            },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: BridgeQueueUpcomingSnapshot? = lock.withLock {
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

    func deliver(revision: UInt64, offset: UInt64, entryId: String) {
        let snapshot = BridgeQueueUpcomingSnapshot(
            revision: revision,
            windows: [
                BridgeQueueUpcomingWindow(
                    window: BridgeLibraryPageWindow(offset: offset, limit: 1),
                    entries: [
                        BridgeQueueEntry(
                            entryId: entryId,
                            trackId: "track-\(entryId)",
                            title: "Track Title",
                            artistNames: "Artist Name",
                            durationClock: nil,
                            albumTitle: "Album Title",
                            coverImage: nil
                        )
                    ]
                )
            ]
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

private func libraryContext(upcomingTotal: Int) -> BridgePlaybackContext {
    BridgePlaybackContext(
        kind: .library,
        sourceTitle: nil,
        shuffled: false,
        upcoming: [],
        upcomingTotal: UInt64(upcomingTotal)
    )
}

private func contextSnapshot(revision: UInt64) -> BridgeQueueSnapshot {
    BridgeQueueSnapshot(
        manual: [],
        context: libraryContext(upcomingTotal: 500),
        hasNext: true,
        hasPrevious: false,
        revision: revision
    )
}

@MainActor
private func waitUntil(_ predicate: @MainActor () -> Bool) async {
    for _ in 0..<500 {
        if predicate() { return }
        await Task.yield()
    }
}

@Suite("PlaybackStore upcoming windows")
struct PlaybackStoreUpcomingWindowTests {
    @MainActor
    @Test("scrolling the queue moves one upcoming read's windows")
    func scrollingMovesOneRead() async {
        let feed = UpcomingFeed()
        let queue = Queue(subscribeUpcoming: { feed.query() })
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot(revision: 1))

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
        "an upcoming value shows only at the queue revision it was sliced from"
    )
    func upcomingFollowsQueueRevision() async {
        let feed = UpcomingFeed()
        let queue = Queue(subscribeUpcoming: { feed.query() })
        let store = PlaybackStore()
        store.applyQueueSnapshot(contextSnapshot(revision: 1))
        await store.loadUpcomingRange(offset: 100, limit: 60, queue: queue)

        feed.deliver(revision: 1, offset: 100, entryId: "first")
        await waitUntil { store.upcomingItem(at: 100) != nil }
        #expect(store.upcomingItem(at: 100)?.entryId == "first")

        // The value for the next revision lands before the queue value does.
        feed.deliver(revision: 2, offset: 100, entryId: "second")
        await waitUntil { store.upcomingItem(at: 100)?.entryId != "first" }
        #expect(
            store.upcomingItem(at: 100)?.entryId == "first",
            "a value ahead of the queue on screen waits for it"
        )

        store.applyQueueSnapshot(contextSnapshot(revision: 2))
        #expect(store.upcomingItem(at: 100)?.entryId == "second")

        store.applyQueueSnapshot(contextSnapshot(revision: 3))
        #expect(
            store.upcomingItem(at: 100) == nil,
            "a queue revision the read has not reached shows no stale entries"
        )
        #expect(feed.openedCount == 1)
    }
}

@Suite("PlaybackStore loading transition")
struct PlaybackStoreBeginLoadingTests {
    /// Core's first `PlaybackLoading` carries only a track id. The now-playing
    /// bar / expanded player read `nowPlaying.track`; if it went nil during the
    /// gap the bar would tear down (dismissing the expanded cover) on every
    /// transition. The prior track must stay until the target's metadata lands.
    @MainActor
    @Test("retains the playing track until the target metadata arrives")
    func retainsPlayingTrack() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.beginLoading(trackId: "b")

        #expect(store.nowPlaying.track?.trackId == "a")
        #expect(store.nowPlaying.isActive)
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    @MainActor
    @Test("retains the paused track until the target metadata arrives")
    func retainsPausedTrack() {
        let store = PlaybackStore()
        store.pause(track: makeTrack("a"), reason: .manual)

        store.beginLoading(trackId: "b")

        #expect(store.nowPlaying.track?.trackId == "a")
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    /// The second `PlaybackLoading` (metadata resolved) swaps the displayed
    /// track from the prior one to the target while audio is still loading.
    @MainActor
    @Test("switches to the target once its metadata lands")
    func switchesToTarget() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.beginLoading(trackId: "b")
        #expect(store.nowPlaying.track?.trackId == "a")

        store.setLoadingTarget(trackId: "b", target: makeTrack("b"))
        #expect(store.nowPlaying.track?.trackId == "b")
        #expect(store.nowPlaying.loadingTrackId == "b")
        #expect(store.nowPlaying.isActive)
    }

    /// A target event for a track that is no longer the loading target (a fast
    /// switch moved on) is ignored — it must not overwrite the current loading.
    @MainActor
    @Test("ignores a target for a stale track id")
    func ignoresStaleTarget() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))
        store.beginLoading(trackId: "b")

        store.setLoadingTarget(trackId: "stale", target: makeTrack("stale"))

        #expect(store.nowPlaying.loadingTrackId == "b")
        #expect(store.nowPlaying.track?.trackId == "a")
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

    /// A seek emits a single resolved `PlaybackLoading` for the *current* track
    /// while it is playing or paused — no bare loading first. Re-enter loading
    /// so the transport shows the buffering spinner while core fills the seek
    /// target, keeping the same track on screen.
    @MainActor
    @Test("a seek while playing re-enters loading for the current track")
    func seekWhilePlayingEntersLoading() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.setLoadingTarget(trackId: "a", target: makeTrack("a"))

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.trackId == "a")
        #expect(store.nowPlaying.isActive)
    }

    @MainActor
    @Test("a seek while paused re-enters loading for the current track")
    func seekWhilePausedEntersLoading() {
        let store = PlaybackStore()
        store.pause(track: makeTrack("a"), reason: .manual)

        store.setLoadingTarget(trackId: "a", target: makeTrack("a"))

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.trackId == "a")
    }

    /// A resolved target for a track other than the one playing — with no bare
    /// loading first — is stale: a faster switch owns the bar, so drop it.
    @MainActor
    @Test("ignores a resolved target for a track other than the one playing")
    func ignoresTargetForOtherPlayingTrack() {
        let store = PlaybackStore()
        store.play(track: makeTrack("a"))

        store.setLoadingTarget(trackId: "b", target: makeTrack("b"))

        #expect(store.nowPlaying.loadingTrackId == nil)
        #expect(store.nowPlaying.track?.trackId == "a")
    }
}

@Suite("NowPlaying state")
struct NowPlayingStateTests {
    @Test("loading counts as playing so the transport shows the pause glyph")
    func loadingIsPlaying() {
        #expect(
            NowPlaying.loading(trackId: "x", target: nil, previous: nil)
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
        titleKey: "core.playback.pause.side_ended.title",
        sideLabel: "A"
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
            durationMs: 0,
            progress: 0.1
        )

        #expect(store.projectSeek(ratio: 0.75) == nil)
        expectPosition(
            store.playbackPositionEvent,
            progress: 0.1,
            positionMs: 10_000,
            durationMs: 0
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
    durationMs: UInt64
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
    durationMs: UInt64
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
        durationMs: UInt64,
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
        durationMs: UInt64,
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
