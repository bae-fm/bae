import Foundation

/// Playback position broadcast by `PlaybackStore.playbackPositionPublisher`.
///
/// Position ticks arrive at display rate during playback — far too frequent
/// for `@Observable` to drive without thrashing the view tree. The store
/// publishes them as a Combine signal so only the progress bar reacts.
/// `.position` carries the raw milliseconds and the `[0,1]` ratio; the seek bar
/// renders its two clocks from them through core's projection, which is also
/// where the elapsed-vs-remaining choice is made — the store cannot make it,
/// since the choice is a config the store does not read. `.reset` clears the bar
/// when playback stops.
public enum PlaybackPositionEvent {
    case position(progress: Double, positionMs: UInt64, durationMs: UInt64)
    case reset
}

public struct PlaybackPositionSnapshot {
    public let positionMs: UInt64
    public let durationMs: UInt64
    public let progress: Double

    public var event: PlaybackPositionEvent {
        .position(
            progress: progress,
            positionMs: positionMs,
            durationMs: durationMs
        )
    }
}
