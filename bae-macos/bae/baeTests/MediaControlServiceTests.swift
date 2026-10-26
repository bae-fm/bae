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
        let service = MediaControlService()

        service.applyMediaControlValues(
            mediaControlValues(
                playback: .preview(
                    target: previewTarget,
                    durationMs: 120_000,
                    isPlaying: true
                )
            ),
            appHandle: fakeAppHandle
        )
        #expect(
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Preview Track.flac"
        )

        service.applyMediaControlValues(
            mediaControlValues(playback: libraryPlayback(trackId: "track-1")),
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
        let service = MediaControlService()

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
        let service = MediaControlService()

        // Enable the scrubber, then clear — `clearNowPlaying` disables it. A
        // preview started afterwards, with its always-known duration, must turn
        // it back on, or preview scrubbing would stay disabled.
        service.updatePosition(positionMs: 1_000, durationMs: 60_000)
        #expect(scrubberEnabled)
        service.clearNowPlaying()
        #expect(!scrubberEnabled)

        service.applyMediaControlValues(
            mediaControlValues(
                playback: .preview(
                    target: previewTarget,
                    durationMs: 90_000,
                    isPlaying: true
                )
            ),
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
        let service = MediaControlService()

        service.applyMediaControlValues(
            mediaControlValues(playback: libraryPlayback(trackId: "track-1")),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            libraryPositions(trackId: "track-2", positionMs: 30_000)
        )
        #expect(elapsedSeconds == nil)

        service.applyMediaControlValues(
            mediaControlValues(playback: libraryPlayback(trackId: "track-2")),
            appHandle: fakeAppHandle
        )
        #expect(elapsedSeconds == 30.0)
    }

    @Test("pausing a preview keeps its elapsed time")
    func pausingAPreviewKeepsItsElapsedTime() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let service = MediaControlService()

        service.applyMediaControlValues(
            mediaControlValues(
                playback: .preview(
                    target: previewTarget,
                    durationMs: 120_000,
                    isPlaying: true
                )
            ),
            appHandle: fakeAppHandle
        )
        service.applyPositions(
            previewPositions(target: previewTarget, positionMs: 45_000)
        )
        #expect(elapsedSeconds == 45.0)

        service.applyMediaControlValues(
            mediaControlValues(
                playback: .preview(
                    target: previewTarget,
                    durationMs: 120_000,
                    isPlaying: false
                )
            ),
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
        let service = MediaControlService()

        service.applyMediaControlValues(
            mediaControlValues(
                playback: .preview(
                    target: previewTarget,
                    durationMs: 120_000,
                    isPlaying: true
                )
            ),
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
}

private let fakeAppHandle = AppHandle(noHandle: AppHandle.NoHandle())

private let previewTarget = BridgePreviewTarget(
    path: "/tmp/Preview Track.flac",
    startSample: 0,
    endSample: nil
)

private func libraryPlayback(trackId: String) -> BridgeMediaControlPlayback {
    .library(
        state: .playing(
            track: BridgeNowPlayingTrack(
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

private func mediaControlValues(
    playback: BridgeMediaControlPlayback
) -> BridgeMediaControlValues {
    BridgeMediaControlValues(
        playback: playback,
        volume: 1,
        isMuted: false
    )
}
