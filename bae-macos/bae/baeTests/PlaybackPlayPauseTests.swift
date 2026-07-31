import BaeKit
import Foundation
import Testing

/// Pins the play/pause press mapping the deleted core `TogglePlayPause`
/// dispatched on the slot: playing or loading pauses, paused resumes, stopped
/// does nothing (no track to act on, and `resume` would tear down an active
/// import preview). The caller computes the absolute command from the
/// `NowPlaying` it renders.
@Suite("Playback.playPause(for:)")
struct PlaybackPlayPauseTests {
    private final class Counter: @unchecked Sendable {
        private let lock = NSLock()
        private var value = 0

        func increment() {
            lock.lock()
            value += 1
            lock.unlock()
        }

        var count: Int {
            lock.lock()
            defer { lock.unlock() }
            return value
        }
    }

    private static func track() -> NowPlayingTrack {
        NowPlayingTrack(
            trackId: "track-1",
            trackTitle: "Track Title",
            artistNames: "Artist Name",
            albumId: "album-1",
            coverImage: nil,
            durationMs: 180_000
        )
    }

    /// Returns the (pause, resume) call counts after one `playPause(for:)`.
    private static func dispatch(_ nowPlaying: NowPlaying) -> (pause: Int, resume: Int) {
        let pauses = Counter()
        let resumes = Counter()
        let playback = Playback(
            pause: { pauses.increment() },
            resume: { resumes.increment() }
        )
        playback.playPause(for: nowPlaying)
        return (pauses.count, resumes.count)
    }

    @Test("playing pauses")
    func playingPauses() {
        let counts = Self.dispatch(.playing(Self.track()))
        #expect(counts == (pause: 1, resume: 0))
    }

    @Test("loading pauses")
    func loadingPauses() {
        let counts = Self.dispatch(
            .loading(trackId: "track-1", target: Self.track(), previous: nil)
        )
        #expect(counts == (pause: 1, resume: 0))
    }

    @Test("paused resumes")
    func pausedResumes() {
        let counts = Self.dispatch(.paused(Self.track(), reason: .manual))
        #expect(counts == (pause: 0, resume: 1))
    }

    @Test("stopped does nothing")
    func stoppedDoesNothing() {
        let counts = Self.dispatch(.stopped)
        #expect(counts == (pause: 0, resume: 0))
    }
}
