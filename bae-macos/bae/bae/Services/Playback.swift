import Foundation

/// Playback transport — commands the now-playing track stream.
/// Narrow subset of `AppHandle` covering play/pause, seek, volume,
/// repeat mode, and the "play this release" trigger. Views that drive
/// transport take this instead of the full `AppService`.
final class Playback: Sendable, Observable {
    let togglePlayPause: @Sendable () -> Void
    let pause: @Sendable () -> Void
    let resume: @Sendable () -> Void
    let nextTrack: @Sendable () -> Void
    let previousTrack: @Sendable () -> Void
    let seekByRatio: @Sendable (_ ratio: Double) -> Void
    let setVolume: @Sendable (_ volume: Float) -> Void
    let toggleMute: @Sendable () -> Void
    let cycleRepeatMode: @Sendable () -> Void
    let playRelease:
        @Sendable (
            _ releaseId: String, _ startTrackIndex: UInt32?, _ shuffle: Bool
        ) -> Void

    init(
        togglePlayPause: @escaping @Sendable () -> Void = {},
        pause: @escaping @Sendable () -> Void = {},
        resume: @escaping @Sendable () -> Void = {},
        nextTrack: @escaping @Sendable () -> Void = {},
        previousTrack: @escaping @Sendable () -> Void = {},
        seekByRatio: @escaping @Sendable (Double) -> Void = { _ in },
        setVolume: @escaping @Sendable (Float) -> Void = { _ in },
        toggleMute: @escaping @Sendable () -> Void = {},
        cycleRepeatMode: @escaping @Sendable () -> Void = {},
        playRelease: @escaping @Sendable (String, UInt32?, Bool) -> Void = {
            _,
            _,
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
        self.playRelease = playRelease
    }

    convenience init(handle: any AppHandleProtocol) {
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
            playRelease: {
                handle.playRelease(
                    releaseId: $0,
                    startTrackIndex: $1,
                    shuffle: $2
                )
            }
        )
    }

    // periphery:ignore
    static let stub = Playback()
}
