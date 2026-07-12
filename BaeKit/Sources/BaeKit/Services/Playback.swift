import Foundation

/// Playback transport — commands the now-playing track stream.
/// Narrow subset of `AppHandle` covering play/pause, seek, volume,
/// repeat mode, the "play this release" trigger, and the pause-between-sides
/// preference. Views that drive transport take this instead of the full
/// `AppService`.
public final class Playback: Sendable, Observable {
    public let pause: @Sendable () -> Void
    public let resume: @Sendable () -> Void
    public let nextTrack: @Sendable () -> Void
    public let previousTrack: @Sendable () -> Void
    public let seekByRatio: @Sendable (_ ratio: Double) -> Void
    public let setVolume: @Sendable (_ volume: Float) -> Void
    public let setMuted: @Sendable (_ muted: Bool) -> Void
    /// Set the repeat mode to an absolute value. Every caller sends the mode it
    /// wants; a cycling button passes `mode.next` computed from what it renders.
    public let setRepeatMode: @Sendable (_ mode: BridgeRepeatMode) -> Void
    public let playRelease:
        @Sendable (
            _ releaseId: String, _ startTrackIndex: UInt32?, _ shuffle: Bool
        ) -> Void
    /// Play several releases as one context, concatenated in the given order.
    /// A single release behaves exactly like `playRelease`; core skips any
    /// release whose tracks can't be loaded.
    public let playReleases: @Sendable (_ releaseIds: [String]) -> Void
    /// Play the whole library in a freshly seeded shuffle. An empty library is a
    /// no-op (logged in core).
    public let playLibraryShuffled: @Sendable () -> Void
    public let setPauseBetweenSides: @Sendable (_ enabled: Bool) throws -> Void

    public init(
        pause: @escaping @Sendable () -> Void = {},
        resume: @escaping @Sendable () -> Void = {},
        nextTrack: @escaping @Sendable () -> Void = {},
        previousTrack: @escaping @Sendable () -> Void = {},
        seekByRatio: @escaping @Sendable (Double) -> Void = { _ in },
        setVolume: @escaping @Sendable (Float) -> Void = { _ in },
        setMuted: @escaping @Sendable (Bool) -> Void = { _ in },
        setRepeatMode: @escaping @Sendable (BridgeRepeatMode) -> Void = {
            _ in
        },
        playRelease: @escaping @Sendable (String, UInt32?, Bool) -> Void = {
            _,
            _,
            _ in
        },
        playReleases: @escaping @Sendable ([String]) -> Void = { _ in },
        playLibraryShuffled: @escaping @Sendable () -> Void = {},
        setPauseBetweenSides: @escaping @Sendable (Bool) throws -> Void = {
            _ in
        }
    ) {
        self.pause = pause
        self.resume = resume
        self.nextTrack = nextTrack
        self.previousTrack = previousTrack
        self.seekByRatio = seekByRatio
        self.setVolume = setVolume
        self.setMuted = setMuted
        self.setRepeatMode = setRepeatMode
        self.playRelease = playRelease
        self.playReleases = playReleases
        self.playLibraryShuffled = playLibraryShuffled
        self.setPauseBetweenSides = setPauseBetweenSides
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            pause: { handle.pause() },
            resume: { handle.resume() },
            nextTrack: { handle.nextTrack() },
            previousTrack: { handle.previousTrack() },
            seekByRatio: { handle.seekByRatio(ratio: $0) },
            setVolume: { handle.setVolume(volume: $0) },
            setMuted: { handle.setMuted(muted: $0) },
            setRepeatMode: { handle.setRepeatMode(mode: $0) },
            playRelease: {
                handle.playRelease(
                    releaseId: $0,
                    startTrackIndex: $1,
                    shuffle: $2
                )
            },
            playReleases: { handle.playReleases(releaseIds: $0) },
            playLibraryShuffled: { handle.playLibraryShuffled() },
            setPauseBetweenSides: {
                try handle.setPauseBetweenSides(enabled: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static let stub = Playback()
    #endif
}

extension Playback {
    /// Send the absolute transport command for a play/pause press: pause
    /// while playing or loading (the transport shows the pause glyph through
    /// a load), resume while paused, nothing while stopped — there is no
    /// track to act on, and `resume` would also tear down an active import
    /// preview.
    public func playPause(for nowPlaying: NowPlaying) {
        switch nowPlaying {
        case .playing, .loading:
            pause()
        case .paused:
            resume()
        case .stopped:
            break
        }
    }
}
