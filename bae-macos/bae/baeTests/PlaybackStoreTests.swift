import Testing

@testable import bae

private func makeTrack(_ id: String) -> NowPlayingTrack {
    NowPlayingTrack(
        trackId: id,
        trackTitle: "Title \(id)",
        artistNames: "Artist Name",
        albumId: "album-1",
        coverImageId: nil,
        durationMs: 0,
        durationLabel: ""
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
        store.nowPlaying = .paused(makeTrack("a"))

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
        store.nowPlaying = .paused(makeTrack("a"))

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
        #expect(NowPlaying.paused(makeTrack("a")).loadingTrackId == nil)
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
