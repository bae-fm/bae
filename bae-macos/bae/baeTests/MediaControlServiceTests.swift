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
                    positionMs: 10_000,
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
            mediaControlValues(playback: libraryPlayback),
            appHandle: fakeAppHandle
        )

        #expect(
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Track Title"
        )
    }

    @Test("a zero duration drops the length and disables the scrubber")
    func unknownDurationDisablesScrubber() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = [:]
        defer { infoCenter.nowPlayingInfo = nil }
        let service = MediaControlService()

        service.updatePosition(positionMs: 5_000, durationMs: 60_000)
        #expect(scrubberEnabled)

        // Core reports 0 when the length isn't known yet; the scrubber must go
        // away so a drag can't map onto a missing timeline.
        service.updatePosition(positionMs: 6_000, durationMs: 0)

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
                    positionMs: 10_000,
                    isPlaying: true
                )
            ),
            appHandle: fakeAppHandle
        )
        #expect(scrubberEnabled)
    }
}

private let fakeAppHandle = AppHandle(noHandle: AppHandle.NoHandle())

private let previewTarget = BridgePreviewTarget(
    path: "/tmp/Preview Track.flac",
    startSample: 0,
    endSample: nil
)

private let libraryPlayback = BridgeMediaControlPlayback.library(
    state: .playing(
        trackId: "track-1",
        trackTitle: "Track Title",
        artistNames: "Artist Name",
        artistId: "artist-1",
        albumId: "album-1",
        albumTitle: "Album Title",
        coverImage: nil,
        durationMs: 200_000
    ),
    position: BridgePlaybackPosition(
        trackId: "track-1",
        positionMs: 30_000,
        durationMs: 200_000,
        progress: 0.15
    ),
    seekRevision: 0
)

private func mediaControlValues(
    playback: BridgeMediaControlPlayback
) -> BridgeMediaControlValues {
    BridgeMediaControlValues(
        playback: playback,
        volume: 1,
        isMuted: false
    )
}
