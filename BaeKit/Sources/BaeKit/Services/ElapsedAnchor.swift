import Foundation

/// Where Now Playing's elapsed time was last set, at what playback rate, and
/// when. The system moves the elapsed time forward on its own from these;
/// core's timeline says when playback leaves that line, and each write of
/// Now Playing carries the elapsed time where the line is then.
struct ElapsedAnchor {
    let positionMs: UInt64
    /// 1.0 while playing, 0.0 otherwise.
    let rate: Double
    let at: ContinuousClock.Instant

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
            at: now
        )
    }
}
