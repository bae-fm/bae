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

    @Test("preview idle clears Now Playing")
    func previewIdleClearsNowPlaying() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer {
            infoCenter.nowPlayingInfo = nil
        }
        let service = MediaControlService()

        service.updateNowPlayingForPreview(
            state: .playing(
                target: BridgePreviewTarget(
                    path: "/tmp/Preview Track.flac",
                    startSample: 0,
                    endSample: nil
                ),
                durationMs: 120_000
            )
        )
        #expect(infoCenter.nowPlayingInfo != nil)

        service.updateNowPlayingForPreview(state: .idle)

        #expect(infoCenter.nowPlayingInfo == nil)
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

        service.updateNowPlayingForPreview(
            state: .playing(
                target: BridgePreviewTarget(
                    path: "/tmp/Preview Track.flac",
                    startSample: 0,
                    endSample: nil
                ),
                durationMs: 90_000
            )
        )
        #expect(scrubberEnabled)
    }
}
