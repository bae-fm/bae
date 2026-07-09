import BaeKit
import MediaPlayer
import Testing

/// Exercises the parts of `MediaControlService` that read and write the
/// process-global Now Playing info center and remote command center. These
/// singletons are shared across the process, so the suite is serialized and
/// each test resets `nowPlayingInfo` on exit.
///
/// Not covered here (needs a live system service, not a unit seam):
///   - `updateNowPlaying(state:appHandle:)`: builds the metadata dict but takes
///     an `AppHandle`, which only a real opened library produces, and the
///     artwork path calls into the bridge.
///
/// The session lifecycle, the interruption handlers, and the
/// `.loading`-counts-as-playing latch are covered in
/// `PlaybackAudioSessionTests.swift`, driven through the public `Playback` spy
/// seam and posted `AVAudioSession` notifications.
@MainActor
@Suite("MediaControlService", .serialized)
struct MediaControlServiceTests {
    private var scrubberEnabled: Bool {
        MPRemoteCommandCenter.shared().changePlaybackPositionCommand.isEnabled
    }

    @Test("updatePosition maps elapsed + duration and enables the scrubber")
    func updatePositionMapsAndEnablesScrubber() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = [:]
        defer { infoCenter.nowPlayingInfo = nil }
        let service = MediaControlService()

        service.updatePosition(positionMs: 30_000, durationMs: 120_000)

        let info = infoCenter.nowPlayingInfo
        #expect(info?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double == 30.0)
        #expect(info?[MPMediaItemPropertyPlaybackDuration] as? Double == 120.0)
        #expect(scrubberEnabled)
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
        #expect(info?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double == 6.0)
        #expect(info?[MPMediaItemPropertyPlaybackDuration] == nil)
        #expect(!scrubberEnabled)
    }

    @Test("updatePosition is a no-op when nothing is playing")
    func updatePositionNoOpWhenCleared() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let service = MediaControlService()

        service.updatePosition(positionMs: 1_000, durationMs: 60_000)

        #expect(infoCenter.nowPlayingInfo == nil)
    }

    @Test("updateCommandAvailability toggles next/previous independently")
    func commandAvailabilityToggles() {
        let center = MPRemoteCommandCenter.shared()
        let service = MediaControlService()

        service.updateCommandAvailability(hasNext: true, hasPrevious: false)
        #expect(center.nextTrackCommand.isEnabled)
        #expect(!center.previousTrackCommand.isEnabled)

        service.updateCommandAvailability(hasNext: false, hasPrevious: true)
        #expect(!center.nextTrackCommand.isEnabled)
        #expect(center.previousTrackCommand.isEnabled)
    }

    @Test("clearNowPlaying wipes info and disables every transport command")
    func clearNowPlayingResetsEverything() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = [:]
        defer { infoCenter.nowPlayingInfo = nil }
        let center = MPRemoteCommandCenter.shared()
        let service = MediaControlService()

        service.updateCommandAvailability(hasNext: true, hasPrevious: true)
        service.updatePosition(positionMs: 1_000, durationMs: 60_000)
        #expect(scrubberEnabled)

        service.clearNowPlaying()

        #expect(infoCenter.nowPlayingInfo == nil)
        #expect(!center.nextTrackCommand.isEnabled)
        #expect(!center.previousTrackCommand.isEnabled)
        #expect(!scrubberEnabled)
    }
}
