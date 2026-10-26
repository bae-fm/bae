import BaeKit
import MediaPlayer
import Testing

/// Exercises the macOS-facing parts of the shared `MediaControlService` — the
/// import-preview session and the scrubber, which is enabled only while a track
/// duration is known. These read and write process-global singletons (Now
/// Playing info + remote command center), so the suite is serialized and each
/// test resets state on exit.
@MainActor
@Suite("MediaControlService", .serialized)
struct MediaControlServiceTests {
    private var scrubberEnabled: Bool {
        MPRemoteCommandCenter.shared().changePlaybackPositionCommand.isEnabled
    }

    @Test("preview owns Now Playing only while active")
    func previewOwnsNowPlayingOnlyWhileActive() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            previewValues(durationMs: 120_000, isPlaying: true),
            appHandle: fakeAppHandle
        )
        #expect(
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Preview Track.flac"
        )

        service.applyValues(
            libraryValues(playingState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )

        #expect(
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Track Title"
        )
    }

    @Test("an unknown duration drops the length and disables the scrubber")
    func unknownDurationDisablesScrubber() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = [:]
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.updatePosition(positionMs: 5_000, durationMs: 60_000)
        #expect(scrubberEnabled)

        // Core reports no duration when the track's length is unknown; the
        // scrubber must go away so a drag can't map onto a missing timeline.
        service.updatePosition(positionMs: 6_000, durationMs: nil)

        let info = infoCenter.nowPlayingInfo
        #expect(
            info?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double == 6.0
        )
        #expect(info?[MPMediaItemPropertyPlaybackDuration] == nil)
        #expect(!scrubberEnabled)
    }

    @Test("a preview started after a clear re-enables the scrubber")
    func previewAfterClearReenablesScrubber() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = [:]
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        // Enable the scrubber, then clear — `clearNowPlaying` disables it. A
        // preview started afterwards, with its always-known duration, must turn
        // it back on, or preview scrubbing would stay disabled.
        service.updatePosition(positionMs: 1_000, durationMs: 60_000)
        #expect(scrubberEnabled)
        service.clearNowPlaying()
        #expect(!scrubberEnabled)

        service.applyValues(
            previewValues(durationMs: 90_000, isPlaying: true),
            appHandle: fakeAppHandle
        )
        #expect(scrubberEnabled)
    }

    @Test(
        "a position that comes before its track's values shows with the track"
    )
    func positionBeforeItsTrackShowsWithTheTrack() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            libraryValues(playingState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            libraryPositions(trackId: "track-2", positionMs: 30_000)
        )
        #expect(elapsedSeconds == nil)

        service.applyValues(
            libraryValues(playingState(trackId: "track-2")),
            appHandle: fakeAppHandle
        )
        #expect(elapsedSeconds == 30.0)
    }

    @Test("pausing a preview keeps its elapsed time")
    func pausingAPreviewKeepsItsElapsedTime() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            previewValues(durationMs: 120_000, isPlaying: true),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            previewPositions(target: previewTarget, positionMs: 45_000)
        )
        #expect(elapsedSeconds == 45.0)

        service.applyValues(
            previewValues(durationMs: 120_000, isPlaying: false),
            appHandle: fakeAppHandle
        )
        #expect(elapsedSeconds == 45.0)
        #expect(
            infoCenter.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackRate]
                as? Double == 0.0
        )
    }

    @Test("another clip's position is not shown on a preview")
    func anotherClipsPositionIsNotShown() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            previewValues(durationMs: 120_000, isPlaying: true),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            previewPositions(
                target: BridgePreviewTarget(
                    path: "/tmp/Other Track.flac",
                    startSample: 0,
                    endSample: nil
                ),
                positionMs: 45_000
            )
        )
        #expect(elapsedSeconds == nil)
    }

    @Test("positions along the playing line are not written again")
    func positionsAlongThePlayingLineAreNotWritten() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            libraryValues(playingState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            libraryPositions(trackId: "track-1", positionMs: 10_000)
        )
        #expect(elapsedSeconds == 10.0)

        clock.advance(ms: 200)
        service.applyPositions(
            libraryPositions(trackId: "track-1", positionMs: 10_200)
        )
        #expect(elapsedSeconds == 10.0)

        clock.advance(ms: 200)
        service.applyValues(
            libraryValues(playingState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        #expect(elapsedSeconds == 10.0)
    }

    @Test("a seek, or a position off the playing line, is written")
    func aSeekOrAPositionOffTheLineIsWritten() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            libraryValues(playingState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            libraryPositions(trackId: "track-1", positionMs: 10_000)
        )

        clock.advance(ms: 200)
        service.applyPositions(
            libraryPositions(
                trackId: "track-1",
                positionMs: 10_500,
                seekRevision: 1
            )
        )
        #expect(elapsedSeconds == 10.5)

        // Playback stalled for the five seconds the line moved on.
        clock.advance(ms: 5_000)
        service.applyPositions(
            libraryPositions(
                trackId: "track-1",
                positionMs: 10_700,
                seekRevision: 1
            )
        )
        #expect(elapsedSeconds == 10.7)
    }

    @Test("pausing writes the elapsed time where playback paused")
    func pausingWritesTheElapsedTimeWherePlaybackPaused() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let clock = TestClock()
        let service = MediaControlService(now: clock.now)

        service.applyValues(
            libraryValues(playingState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            libraryPositions(trackId: "track-1", positionMs: 10_000)
        )

        clock.advance(ms: 2_000)
        service.applyValues(
            libraryValues(pausedState(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        #expect(elapsedSeconds == 12.0)
        #expect(
            infoCenter.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackRate]
                as? Double == 0.0
        )
    }
}

/// A clock that moves only when a test moves it, so the elapsed time Now
/// Playing is given can be compared exactly.
private final class TestClock: @unchecked Sendable {
    private var instant = ContinuousClock.now

    var now: @Sendable () -> ContinuousClock.Instant {
        { [self] in instant }
    }

    func advance(ms: Int) {
        instant += .milliseconds(ms)
    }
}

private func pausedState(trackId: String) -> BridgePlaybackValueState {
    .paused(track: nowPlayingTrack(trackId: trackId), reason: .manual)
}

private func libraryPositions(
    trackId: String,
    positionMs: UInt64,
    seekRevision: UInt64
) -> BridgePlaybackPositions {
    let positions = libraryPositions(trackId: trackId, positionMs: positionMs)
    return BridgePlaybackPositions(
        library: positions.library,
        seekRevision: seekRevision,
        preview: positions.preview,
        mediaControl: positions.mediaControl
    )
}

private let fakeAppHandle = AppHandle(noHandle: AppHandle.NoHandle())

private let previewTarget = BridgePreviewTarget(
    path: "/tmp/Preview Track.flac",
    startSample: 0,
    endSample: nil
)

private func playingState(trackId: String) -> BridgePlaybackValueState {
    .playing(track: nowPlayingTrack(trackId: trackId))
}

private func nowPlayingTrack(trackId: String) -> BridgeNowPlayingTrack {
    BridgeNowPlayingTrack(
        track: BridgePlayingTrack(
            trackId: trackId,
            durationMs: 200_000
        ),
        display: BridgeTrackDisplay(
            title: "Track Title",
            artistNames: "Artist Name",
            albumId: "album-1",
            releaseId: "release-1",
            albumTitle: "Album Title",
            coverImage: nil
        )
    )
}

private func libraryPositions(
    trackId: String,
    positionMs: UInt64
) -> BridgePlaybackPositions {
    BridgePlaybackPositions(
        library: BridgePlaybackPosition(
            trackId: trackId,
            positionMs: Int64(positionMs),
            durationMs: 200_000,
            progress: Double(positionMs) / 200_000
        ),
        seekRevision: 0,
        preview: nil,
        mediaControl: BridgeMediaControlPosition(
            trackId: trackId,
            positionMs: positionMs,
            durationMs: 200_000,
            progress: Double(positionMs) / 200_000
        )
    )
}

private func previewPositions(
    target: BridgePreviewTarget,
    positionMs: UInt64
) -> BridgePlaybackPositions {
    BridgePlaybackPositions(
        library: nil,
        seekRevision: 0,
        preview: BridgePreviewPosition(
            target: target,
            durationMs: 120_000,
            positionMs: positionMs,
            progress: Double(positionMs) / 120_000
        ),
        mediaControl: nil
    )
}

private var elapsedSeconds: Double? {
    MPNowPlayingInfoCenter.default().nowPlayingInfo?[
        MPNowPlayingInfoPropertyElapsedPlaybackTime
    ] as? Double
}

/// Values whose media controls show the library playback in `state`.
private func libraryValues(_ state: BridgePlaybackValueState)
    -> BridgePlaybackValues
{
    playbackValues(state: state, preview: .idle, mediaControl: .library)
}

/// Values whose media controls show the preview of `previewTarget`.
private func previewValues(durationMs: UInt64, isPlaying: Bool)
    -> BridgePlaybackValues
{
    playbackValues(
        state: .stopped,
        preview: isPlaying
            ? .playing(target: previewTarget, durationMs: durationMs)
            : .paused(target: previewTarget, durationMs: durationMs),
        mediaControl: .preview(
            target: previewTarget,
            durationMs: durationMs,
            isPlaying: isPlaying
        )
    )
}

private func playbackValues(
    state: BridgePlaybackValueState,
    preview: BridgePreviewState,
    mediaControl: BridgeMediaControlPlayback
) -> BridgePlaybackValues {
    BridgePlaybackValues(
        state: state,
        volume: 1,
        isMuted: false,
        repeatMode: .off,
        remote: .local,
        preview: preview,
        mediaControl: mediaControl
    )
}
