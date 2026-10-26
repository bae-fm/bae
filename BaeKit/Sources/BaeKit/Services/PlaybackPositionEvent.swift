import Foundation

/// Playback position broadcast by `PlaybackStore.playbackPositionPublisher`.
///
/// A position arrives with every tick core sends during playback, several a
/// second — too often for `@Observable` to drive without re-evaluating every
/// view that reads the store. The store publishes them as a Combine signal so
/// only the views that subscribe to it react.
/// `.position` carries the raw milliseconds (a nil duration when the track's
/// length is unknown) and the `[0,1]` ratio; the seek bar
/// renders its two clocks from them through core's projection, which is also
/// where the elapsed-vs-remaining choice is made — the store cannot make it,
/// since the choice is a config the store does not read. `.reset` clears the bar
/// when playback stops.
public enum PlaybackPositionEvent {
    case position(progress: Double, positionMs: Int64, durationMs: UInt64?)
    case reset
}

public struct PlaybackPositionSnapshot {
    public let positionMs: Int64
    /// Nil when the track's length is unknown.
    public let durationMs: UInt64?
    public let progress: Double

    public var event: PlaybackPositionEvent {
        .position(
            progress: progress,
            positionMs: positionMs,
            durationMs: durationMs
        )
    }
}
