import Foundation

/// A position at a moment, and the rate it moves at from there: what core
/// puts on a position's stream instead of the position itself. A native view
/// works out where playback has got to each display frame from the anchor and
/// the frame's time, so nothing is sent as playback plays.
public struct PlaybackAnchor: Sendable, Equatable {
    /// Track-relative at the anchor: negative while pregap audio plays.
    public let positionMs: Int64
    /// How fast the position moves on from the anchor, as a multiple of real
    /// time: zero while paused.
    public let rate: Double
    /// When the position was read, in nanoseconds on the frame clock
    /// (``FrameClock``).
    public let anchoredAtNs: UInt64
    /// `nil` when the length is unknown.
    public let durationMs: UInt64?

    public init(
        positionMs: Int64,
        rate: Double,
        anchoredAtNs: UInt64,
        durationMs: UInt64?
    ) {
        self.positionMs = positionMs
        self.rate = rate
        self.anchoredAtNs = anchoredAtNs
        self.durationMs = durationMs
    }

    /// The position at a frame at `frameNs` on the frame clock: the anchor's
    /// plus the time since it times the rate, held to the length. A frame
    /// timed before the anchor shows the anchor.
    public func positionMs(atFrameNs frameNs: UInt64) -> Int64 {
        let elapsedNs = frameNs > anchoredAtNs ? frameNs - anchoredAtNs : 0
        var position = Double(positionMs) + Double(elapsedNs) / 1e6 * rate
        if let durationMs {
            position = min(position, Double(durationMs))
        }
        return Int64(position.rounded(.down))
    }

    /// The seek bar's fill at that frame: zero throughout when the length is
    /// unknown.
    public func progress(atFrameNs frameNs: UInt64) -> Double {
        guard let durationMs, durationMs > 0 else { return 0 }
        let position = Double(positionMs(atFrameNs: frameNs))
        return max(0, min(1, position / Double(durationMs)))
    }
}

extension BridgeTrackPosition {
    /// The anchor the stream holds.
    public var anchor: PlaybackAnchor {
        PlaybackAnchor(
            positionMs: positionMs,
            rate: rate,
            anchoredAtNs: anchoredAtNs,
            durationMs: durationMs
        )
    }
}

extension BridgePreviewPosition {
    /// The preview's position as an anchor that stands where it is: core
    /// sends the preview's position as it moves.
    public var anchor: PlaybackAnchor {
        PlaybackAnchor(
            positionMs: Int64(positionMs),
            rate: 0,
            anchoredAtNs: 0,
            durationMs: durationMs
        )
    }
}

/// The clock display frames are timed on, which core stamps anchors with
/// (`frame_clock_ns` in bae-util): `CLOCK_UPTIME_RAW`, the clock of
/// `CACurrentMediaTime` and so of a display link's timestamps.
public enum FrameClock {
    /// Now, in nanoseconds.
    public static func nowNs() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_UPTIME_RAW)
    }

    /// A display link's timestamp, in seconds of `CACurrentMediaTime`, in
    /// nanoseconds.
    public static func ns(ofMediaTime seconds: CFTimeInterval) -> UInt64 {
        UInt64(max(0, seconds) * 1e9)
    }
}
