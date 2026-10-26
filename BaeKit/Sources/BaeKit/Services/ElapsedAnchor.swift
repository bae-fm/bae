import Foundation

/// Where Now Playing's elapsed time was last set, at what playback rate, and
/// when. The system moves the elapsed time forward on its own from these, so
/// the elapsed time is written again only when playback leaves the line they
/// draw: a seek, a new track or clip, a rate change, or a position that lands
/// off where the line says it should be.
struct ElapsedAnchor {
    /// How far a position may land from where the anchor puts it before it
    /// counts as a jump: past the jitter of position deliveries, which arrive
    /// a few times a second, and short of any seek a listener would notice.
    static let jumpThresholdMs: Double = 1_000

    let positionMs: UInt64
    /// 1.0 while playing, 0.0 otherwise.
    let rate: Double
    let at: ContinuousClock.Instant
    /// The library seek revision the position was read at; `nil` for a
    /// preview's position, which has none.
    let seekRevision: UInt64?

    /// Where the anchor puts the elapsed time at `now`.
    func positionMs(at now: ContinuousClock.Instant) -> Double {
        let elapsed = now - at
        let elapsedMs =
            Double(elapsed.components.seconds) * 1_000
            + Double(elapsed.components.attoseconds) / 1e15
        return Double(positionMs) + rate * elapsedMs
    }

    /// The same line from `now` on, at `rate`.
    func rebased(rate: Double, at now: ContinuousClock.Instant) -> Self {
        Self(
            positionMs: UInt64(max(0, positionMs(at: now)).rounded()),
            rate: rate,
            at: now,
            seekRevision: seekRevision
        )
    }

    /// Whether `positionMs`, read at `seekRevision`, leaves the line: an
    /// applied seek, or a position past `jumpThresholdMs` from where the line
    /// is at `now`.
    func isJump(
        to positionMs: UInt64,
        seekRevision: UInt64?,
        at now: ContinuousClock.Instant
    ) -> Bool {
        seekRevision != self.seekRevision
            || abs(Double(positionMs) - self.positionMs(at: now))
                > Self.jumpThresholdMs
    }
}
