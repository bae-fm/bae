import Foundation

/// Playback transport — commands the now-playing track stream.
/// Narrow subset of `AppHandle` covering play/pause, seek, volume,
/// repeat mode, the "play this release" trigger, and the pause-between-sides
/// preference. Views that drive transport take this instead of the full
/// `AppService`.
public final class Playback: Sendable, Observable {
    public let togglePlayPause: @Sendable () -> Void
    public let pause: @Sendable () -> Void
    public let resume: @Sendable () -> Void
    public let nextTrack: @Sendable () -> Void
    public let previousTrack: @Sendable () -> Void
    public let seekByRatio: @Sendable (_ ratio: Double) -> Void
    public let setVolume: @Sendable (_ volume: Float) -> Void
    public let toggleMute: @Sendable () -> Void
    public let cycleRepeatMode: @Sendable () -> Void
    /// Set the repeat mode to an absolute value (the menu's Repeat picker). The
    /// now-playing bar's single button cycles instead.
    public let setRepeatMode: @Sendable (_ mode: RepeatMode) -> Void
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
        togglePlayPause: @escaping @Sendable () -> Void = {},
        pause: @escaping @Sendable () -> Void = {},
        resume: @escaping @Sendable () -> Void = {},
        nextTrack: @escaping @Sendable () -> Void = {},
        previousTrack: @escaping @Sendable () -> Void = {},
        seekByRatio: @escaping @Sendable (Double) -> Void = { _ in },
        setVolume: @escaping @Sendable (Float) -> Void = { _ in },
        toggleMute: @escaping @Sendable () -> Void = {},
        cycleRepeatMode: @escaping @Sendable () -> Void = {},
        setRepeatMode: @escaping @Sendable (RepeatMode) -> Void = { _ in },
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
        self.togglePlayPause = togglePlayPause
        self.pause = pause
        self.resume = resume
        self.nextTrack = nextTrack
        self.previousTrack = previousTrack
        self.seekByRatio = seekByRatio
        self.setVolume = setVolume
        self.toggleMute = toggleMute
        self.cycleRepeatMode = cycleRepeatMode
        self.setRepeatMode = setRepeatMode
        self.playRelease = playRelease
        self.playReleases = playReleases
        self.playLibraryShuffled = playLibraryShuffled
        self.setPauseBetweenSides = setPauseBetweenSides
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            togglePlayPause: { handle.togglePlayPause() },
            pause: { handle.pause() },
            resume: { handle.resume() },
            nextTrack: { handle.nextTrack() },
            previousTrack: { handle.previousTrack() },
            seekByRatio: { handle.seekByRatio(ratio: $0) },
            setVolume: { handle.setVolume(volume: $0) },
            toggleMute: { handle.toggleMute() },
            cycleRepeatMode: { handle.cycleRepeatMode() },
            setRepeatMode: { handle.setRepeatMode(mode: $0.bridge) },
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

    // periphery:ignore
    public static let stub = Playback()
}
