import Foundation

/// Playback position broadcast on `PlaybackStore.playbackPositionSubject`.
///
/// Position ticks arrive at display rate during playback — far too frequent
/// for `@Observable` to drive without thrashing the view tree. The store
/// publishes them as a Combine signal so only the progress bar reacts.
/// `.position` carries the elapsed/remaining clock labels (formatted in the
/// reducer from raw ms) and the `[0,1]` ratio; `.reset` clears the bar when
/// playback stops.
public enum PlaybackPositionEvent {
    case position(progress: Double, elapsed: String, remaining: String)
    case reset
}

public struct PlaybackPositionSnapshot {
    public let positionMs: UInt64
    public let durationMs: UInt64
    public let progress: Double

    public var event: PlaybackPositionEvent {
        .position(
            progress: progress,
            elapsed: DurationClock.text(Int64(positionMs)),
            remaining: DurationClock.remaining(
                positionMs: positionMs,
                durationMs: durationMs
            )
        )
    }
}
