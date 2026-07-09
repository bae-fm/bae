import AVFAudio
import BaeKit
import MediaPlayer
import Testing

/// Exercises the parts of `MediaControlService` that only exist on iOS: the
/// `AVAudioSession` lifecycle, the interruption handlers, and the
/// `.loading`-counts-as-playing latch that `updateNowPlaying(state:appHandle:)`
/// drives before every metadata write. These read and write process-global
/// singletons (interruption notifications, the Now Playing info center), so
/// the suite is serialized; each service is scoped to its own test so it's
/// deallocated — and its `NotificationCenter` observers with it — before the
/// next test posts.
@MainActor
@Suite("MediaControlService audio session", .serialized)
struct PlaybackAudioSessionTests {
    private func makeService() -> (MediaControlService, PlaybackSpy) {
        let spy = PlaybackSpy()
        let service = MediaControlService()
        service.setupRemoteCommands(
            playback: spy.playback,
            playbackStore: PlaybackStore()
        )
        return (service, spy)
    }

    private func postInterruptionBegan() {
        NotificationCenter.default.post(
            name: AVAudioSession.interruptionNotification,
            object: nil,
            userInfo: [
                AVAudioSessionInterruptionTypeKey:
                    AVAudioSession.InterruptionType.began.rawValue
            ]
        )
    }

    private func postInterruptionEnded(shouldResume: Bool) {
        var userInfo: [AnyHashable: Any] = [
            AVAudioSessionInterruptionTypeKey:
                AVAudioSession.InterruptionType.ended.rawValue
        ]
        if shouldResume {
            userInfo[AVAudioSessionInterruptionOptionKey] =
                AVAudioSession.InterruptionOptions.shouldResume.rawValue
        }
        NotificationCenter.default.post(
            name: AVAudioSession.interruptionNotification,
            object: nil,
            userInfo: userInfo
        )
    }

    @Test("an interruption while playing pauses core and resumes on ended")
    func interruptionWhilePlayingPausesAndResumes() {
        let (service, spy) = makeService()
        service.updateNowPlaying(state: playingState(), appHandle: fakeAppHandle)

        postInterruptionBegan()
        #expect(spy.pauseCount == 1)

        postInterruptionEnded(shouldResume: true)
        #expect(spy.resumeCount == 1)
    }

    /// The regression case for the `.loading`-counts-as-playing latch: a bare
    /// loading event (no resolved metadata) still latches `lastKnownIsPlaying`,
    /// so an interruption mid-transition pauses core and auto-resumes.
    @Test("an interruption during a track transition pauses and resumes")
    func interruptionDuringTrackTransitionPausesAndResumes() {
        let (service, spy) = makeService()
        service.updateNowPlaying(
            state: .loading(trackId: "t1", track: nil),
            appHandle: fakeAppHandle
        )

        postInterruptionBegan()
        #expect(spy.pauseCount == 1)

        postInterruptionEnded(shouldResume: true)
        #expect(spy.resumeCount == 1)
    }

    @Test("an interruption while paused neither pauses nor resumes")
    func interruptionWhilePausedDoesNothing() {
        let (service, spy) = makeService()
        service.updateNowPlaying(state: pausedState(), appHandle: fakeAppHandle)

        postInterruptionBegan()
        #expect(spy.pauseCount == 0)

        postInterruptionEnded(shouldResume: true)
        #expect(spy.resumeCount == 0)
    }

    @Test("an interruption after stopped does nothing: the shared clear reset the latch")
    func interruptionAfterStoppedDoesNothing() {
        let (service, spy) = makeService()
        service.updateNowPlaying(state: playingState(), appHandle: fakeAppHandle)
        service.updateNowPlaying(state: .stopped, appHandle: fakeAppHandle)

        postInterruptionBegan()
        #expect(spy.pauseCount == 0)

        postInterruptionEnded(shouldResume: true)
        #expect(spy.resumeCount == 0)
    }

    @Test("ended without shouldResume does not resume")
    func endedWithoutShouldResumeDoesNotResume() {
        let (service, spy) = makeService()
        service.updateNowPlaying(state: playingState(), appHandle: fakeAppHandle)

        postInterruptionBegan()
        postInterruptionEnded(shouldResume: false)

        #expect(spy.resumeCount == 0)
    }

    @Test("a resolved loading target appears in the info center at rate 0")
    func resolvedLoadingPushesLockScreenTarget() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let (service, _) = makeService()

        service.updateNowPlaying(
            state: .loading(
                trackId: "t1",
                track: BridgeLoadingTrackInfo(
                    trackTitle: "Target Title",
                    artistNames: "Artist Name",
                    albumId: "album-1",
                    albumTitle: "Album Title",
                    coverImageId: nil,
                    durationMs: 180_000
                )
            ),
            appHandle: fakeAppHandle
        )

        let info = infoCenter.nowPlayingInfo
        #expect(info?[MPMediaItemPropertyTitle] as? String == "Target Title")
        #expect(info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double == 0.0)
    }

    @Test("a bare loading event leaves the prior info in place")
    func bareLoadingLeavesPriorInfo() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let (service, _) = makeService()
        service.updateNowPlaying(state: playingState(), appHandle: fakeAppHandle)
        let priorTitle =
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String

        service.updateNowPlaying(
            state: .loading(trackId: "t1", track: nil),
            appHandle: fakeAppHandle
        )

        #expect(
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == priorTitle
        )
    }

    @Test("stopped clears the info center and disables transport")
    func stoppedClearsInfoAndTransport() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        let center = MPRemoteCommandCenter.shared()
        defer {
            infoCenter.nowPlayingInfo = nil
            center.nextTrackCommand.isEnabled = false
            center.previousTrackCommand.isEnabled = false
        }
        let (service, _) = makeService()
        service.updateNowPlaying(state: playingState(), appHandle: fakeAppHandle)
        service.updateCommandAvailability(hasNext: true, hasPrevious: true)

        service.updateNowPlaying(state: .stopped, appHandle: fakeAppHandle)

        #expect(infoCenter.nowPlayingInfo == nil)
        #expect(!center.nextTrackCommand.isEnabled)
        #expect(!center.previousTrackCommand.isEnabled)
    }
}

// MARK: - Fixtures

private let fakeAppHandle = AppHandle(noHandle: AppHandle.NoHandle())

private func playingState(trackId: String = "t1") -> BridgePlaybackState {
    .playing(
        trackId: trackId,
        trackTitle: "Track Title",
        artistNames: "Artist Name",
        artistId: "artist-1",
        albumId: "album-1",
        albumTitle: "Album Title",
        coverImageId: nil,
        durationMs: 200_000
    )
}

private func pausedState(trackId: String = "t1") -> BridgePlaybackState {
    .paused(
        trackId: trackId,
        trackTitle: "Track Title",
        artistNames: "Artist Name",
        artistId: "artist-1",
        albumId: "album-1",
        albumTitle: "Album Title",
        coverImageId: nil,
        durationMs: 200_000
    )
}

/// Records `Playback` transport calls. `@unchecked Sendable`: the closures run
/// synchronously on the main thread within a single test (`NotificationCenter`
/// delivers posts synchronously to observers registered without a queue).
private final class PlaybackSpy: @unchecked Sendable {
    private(set) var pauseCount = 0
    private(set) var resumeCount = 0

    lazy var playback = Playback(
        pause: { [weak self] in self?.pauseCount += 1 },
        resume: { [weak self] in self?.resumeCount += 1 }
    )
}
