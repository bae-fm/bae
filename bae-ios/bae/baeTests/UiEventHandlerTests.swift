import BaeKit
import Testing

@testable import bae

/// The iOS UI-event router decomposes a flat `BridgeUiEvent` into the narrow
/// transport / state / control buckets before applying it to a store. These
/// suites cover that classification and the Now Playing metadata it builds —
/// the pure, store-independent half of `routeUiEvent`. The apply side
/// (`routeUiEvent` writing into the stores) needs a live `AppService`, which
/// only a real opened library produces, so it isn't unit-tested here.
@Suite("UiEventHandler transport classification")
struct PlaybackTransportEventTests {
    @Test("a Playing event carries the track and builds Now Playing metadata")
    func playingBuildsMetadata() {
        let event = BridgeUiEvent.playbackPlaying(
            trackId: "t1",
            trackTitle: "Track Title",
            artistNames: "Artist Name",
            artistId: "artist-1",
            albumId: "album-1",
            albumTitle: "Album Title",
            coverImageId: "cover-1",
            durationMs: 210_000
        )

        guard case .playing(let update) = PlaybackTransportEvent(event) else {
            Issue.record("Playing event should classify as .playing")
            return
        }

        #expect(update.track.trackId == "t1")
        #expect(update.track.albumId == "album-1")
        #expect(update.albumTitle == "Album Title")

        let metadata = update.metadata(isPlaying: true)
        #expect(metadata.trackTitle == "Track Title")
        // `artistNames` is the display string; the event's separate `artistId`
        // has no metadata slot and is dropped.
        #expect(metadata.artistNames == "Artist Name")
        #expect(metadata.albumTitle == "Album Title")
        #expect(metadata.coverImageId == "cover-1")
        #expect(metadata.durationMs == 210_000)
        #expect(metadata.playbackRate == 1.0)
    }

    @Test("a Paused event carries its pause reason")
    func pausedCarriesReason() {
        let event = BridgeUiEvent.playbackPaused(
            trackId: "t1",
            trackTitle: "Track Title",
            artistNames: "Artist Name",
            artistId: "artist-1",
            albumId: "album-1",
            albumTitle: "Album Title",
            coverImageId: nil,
            durationMs: 0,
            reason: .manual
        )

        guard case .paused(let update, let reason) = PlaybackTransportEvent(event) else {
            Issue.record("Paused event should classify as .paused")
            return
        }
        #expect(update.track.trackId == "t1")
        #expect(update.metadata(isPlaying: false).coverImageId == nil)
        #expect(reason == .manual)
    }

    @Test("Loading without resolved metadata carries only the track id")
    func loadingWithoutMetadata() {
        let event = BridgeUiEvent.playbackLoading(trackId: "t2", track: nil)

        guard case .loading(let trackId, let track) = PlaybackTransportEvent(event) else {
            Issue.record("Loading event should classify as .loading")
            return
        }
        #expect(trackId == "t2")
        #expect(track == nil)
    }

    @Test("Loading with resolved metadata carries the target track info")
    func loadingWithMetadata() {
        let info = BridgeLoadingTrackInfo(
            trackTitle: "Target Title",
            artistNames: "Artist Name",
            albumId: "album-1",
            albumTitle: "Album Title",
            coverImageId: "cover-2",
            durationMs: 180_000
        )
        let event = BridgeUiEvent.playbackLoading(trackId: "t2", track: info)

        guard case .loading(let trackId, let track) = PlaybackTransportEvent(event) else {
            Issue.record("Loading event should classify as .loading")
            return
        }
        #expect(trackId == "t2")
        #expect(track?.trackTitle == "Target Title")
        #expect(track?.durationMs == 180_000)
    }

    @Test("Stopped classifies as .stopped")
    func stoppedClassifies() {
        guard case .stopped = PlaybackTransportEvent(.playbackStopped) else {
            Issue.record("Stopped event should classify as .stopped")
            return
        }
    }

    @Test("non-transport events are not transport events")
    func nonTransportReturnsNil() {
        #expect(PlaybackTransportEvent(.volumeChanged(volume: 0.5)) == nil)
        #expect(
            PlaybackTransportEvent(
                .playbackProgress(
                    trackId: "t1",
                    positionMs: 0,
                    durationMs: 0,
                    progress: 0
                )
            ) == nil
        )
    }
}

@Suite("UiEventHandler state classification")
struct PlaybackStateEventTests {
    @Test("a PlaybackError carries its reason")
    func errorCarriesReason() {
        guard
            case .error(let reason) = PlaybackStateEvent(.playbackError(reason: .syncDisconnected))
        else {
            Issue.record("PlaybackError should classify as .error")
            return
        }
        #expect(reason == .syncDisconnected)
    }

    @Test("Progress carries position, duration, and fraction")
    func progressCarriesFields() {
        let event = BridgeUiEvent.playbackProgress(
            trackId: "t1",
            positionMs: 30_000,
            durationMs: 120_000,
            progress: 0.25
        )
        guard
            case .progress(let trackId, let positionMs, let durationMs, let progress) =
                PlaybackStateEvent(event)
        else {
            Issue.record("Progress should classify as .progress")
            return
        }
        #expect(trackId == "t1")
        #expect(positionMs == 30_000)
        #expect(durationMs == 120_000)
        #expect(progress == 0.25)
    }

    @Test("Seeked classifies distinctly from progress")
    func seekedClassifies() {
        let event = BridgeUiEvent.playbackSeeked(
            trackId: "t1",
            positionMs: 60_000,
            durationMs: 120_000,
            progress: 0.5
        )
        guard case .seeked(_, let positionMs, _, _) = PlaybackStateEvent(event) else {
            Issue.record("Seeked should classify as .seeked")
            return
        }
        #expect(positionMs == 60_000)
    }

    @Test("a transport event is not a state event")
    func transportIsNotState() {
        #expect(PlaybackStateEvent(.playbackStopped) == nil)
    }
}

@Suite("UiEventHandler control classification")
struct PlaybackControlEventTests {
    @Test("Volume carries the level")
    func volumeCarriesLevel() {
        guard case .volume(let volume) = PlaybackControlEvent(.volumeChanged(volume: 0.8)) else {
            Issue.record("VolumeChanged should classify as .volume")
            return
        }
        #expect(volume == 0.8)
    }

    @Test("Mute carries the flag")
    func muteCarriesFlag() {
        guard case .mute(let isMuted) = PlaybackControlEvent(.muteChanged(isMuted: true)) else {
            Issue.record("MuteChanged should classify as .mute")
            return
        }
        #expect(isMuted)
    }

    @Test("RepeatMode carries the bridge mode")
    func repeatModeCarriesMode() {
        guard case .repeatMode(let mode) = PlaybackControlEvent(.repeatModeChanged(mode: .context))
        else {
            Issue.record("RepeatModeChanged should classify as .repeatMode")
            return
        }
        guard case .context = mode else {
            Issue.record("Expected .context, got \(mode)")
            return
        }
    }

    @Test("QueueItemsAdded carries the count")
    func queueItemsAddedCarriesCount() {
        guard case .queueItemsAdded(let count) = PlaybackControlEvent(.queueItemsAdded(count: 3))
        else {
            Issue.record("QueueItemsAdded should classify as .queueItemsAdded")
            return
        }
        #expect(count == 3)
    }

    @Test("a state event is not a control event")
    func stateIsNotControl() {
        #expect(PlaybackControlEvent(.playbackError(reason: .syncDisconnected)) == nil)
    }
}

/// The desktop/preview events the iOS handler receives but does nothing with
/// must fall through every playback classifier, so they reach the ignore path
/// rather than a store apply.
@Suite("UiEventHandler ignores desktop and preview events")
struct UiEventIgnoreTests {
    private static let ignored: [BridgeUiEvent] = [
        .previewIdle,
        .previewPlaying(path: "/tmp/preview.flac", durationMs: 1_000),
        .previewPaused(path: "/tmp/preview.flac", durationMs: 1_000),
        .previewProgress(positionMs: 0, progress: 0),
        .candidateImportLoudnessProgress(
            key: "k",
            tracksDone: 0,
            tracksTotal: 1,
            fraction: 0
        ),
        .releaseTransferProgress(releaseId: "r1", action: .pin),
        .releaseTransferEnded(releaseId: "r1"),
    ]

    @Test("preview and desktop events classify as no playback bucket")
    func ignoredEventsClassifyAsNil() {
        for event in Self.ignored {
            #expect(PlaybackTransportEvent(event) == nil)
            #expect(PlaybackStateEvent(event) == nil)
            #expect(PlaybackControlEvent(event) == nil)
        }
    }
}
