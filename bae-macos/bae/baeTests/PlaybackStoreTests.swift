import Testing

@testable import bae

private func makeTrack(_ id: String) -> NowPlayingTrack {
    NowPlayingTrack(
        trackId: id,
        trackTitle: "Title \(id)",
        artistNames: "Artist Name",
        albumId: "album-1",
        coverImageId: nil,
        durationMs: 0
    )
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
        store.nowPlaying = .playing(makeTrack("a"))

        store.beginLoading(trackId: "b")

        #expect(store.nowPlaying.track?.trackId == "a")
        #expect(store.nowPlaying.isActive)
        #expect(store.nowPlaying.loadingTrackId == "b")
    }

    @MainActor
    @Test("retains the paused track until the target metadata arrives")
    func retainsPausedTrack() {
        let store = PlaybackStore()
        store.nowPlaying = .paused(makeTrack("a"), reason: .manual)

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
        store.nowPlaying = .playing(makeTrack("a"))

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
        store.nowPlaying = .playing(makeTrack("a"))
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
        store.nowPlaying = .playing(makeTrack("a"))

        store.setLoadingTarget(trackId: "a", target: makeTrack("a"))

        #expect(store.nowPlaying.loadingTrackId == "a")
        #expect(store.nowPlaying.track?.trackId == "a")
        #expect(store.nowPlaying.isActive)
    }

    @MainActor
    @Test("a seek while paused re-enters loading for the current track")
    func seekWhilePausedEntersLoading() {
        let store = PlaybackStore()
        store.nowPlaying = .paused(makeTrack("a"), reason: .manual)

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
        store.nowPlaying = .playing(makeTrack("a"))

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
        expectPosition(
            store.playbackPositionSubject.value,
            progress: 0.75,
            elapsed: "1:15",
            remaining: "-0:25"
        )
        let stale = store.updatePlaybackPosition(
            positionMs: 20_000,
            durationMs: 100_000,
            progress: 0.2
        )
        #expect(stale.positionMs == 75_000)
        expectPosition(
            store.playbackPositionSubject.value,
            progress: 0.75,
            elapsed: "1:15",
            remaining: "-0:25"
        )

        let accepted = store.updatePlaybackPosition(
            positionMs: 76_000,
            durationMs: 100_000,
            progress: 0.76
        )

        #expect(accepted.positionMs == 76_000)
        expectPosition(
            store.playbackPositionSubject.value,
            progress: 0.76,
            elapsed: "1:16",
            remaining: "-0:24"
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
            store.playbackPositionSubject.value,
            progress: 0.25,
            elapsed: "0:25",
            remaining: "-1:15"
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
            store.playbackPositionSubject.value,
            progress: 0.25,
            elapsed: "0:25",
            remaining: "-1:15"
        )
        let stale = store.updatePlaybackPosition(
            positionMs: 70_000,
            durationMs: 100_000,
            progress: 0.7
        )
        #expect(stale.positionMs == 25_000)
        expectPosition(
            store.playbackPositionSubject.value,
            progress: 0.25,
            elapsed: "0:25",
            remaining: "-1:15"
        )

        let accepted = store.updatePlaybackPosition(
            positionMs: 25_100,
            durationMs: 100_000,
            progress: 0.251
        )

        #expect(accepted.positionMs == 25_100)
        expectPosition(
            store.playbackPositionSubject.value,
            progress: 0.251,
            elapsed: "0:25",
            remaining: "-1:15"
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
            store.playbackPositionSubject.value,
            progress: 0.25,
            elapsed: "0:25",
            remaining: "-1:15"
        )

        let accepted = store.updatePlaybackPosition(
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
            store.playbackPositionSubject.value,
            progress: 0.1,
            elapsed: "0:10",
            remaining: "-0:00"
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

        guard case .reset = store.playbackPositionSubject.value else {
            Issue.record("position did not reset")
            return
        }
    }
}

private func expectPosition(
    _ event: PlaybackPositionEvent,
    progress: Double,
    elapsed: String,
    remaining: String
) {
    guard
        case .position(
            let actualProgress,
            let actualElapsed,
            let actualRemaining
        ) = event
    else {
        Issue.record("position event was reset")
        return
    }
    #expect(actualProgress == progress)
    #expect(actualElapsed == elapsed)
    #expect(actualRemaining == remaining)
}
