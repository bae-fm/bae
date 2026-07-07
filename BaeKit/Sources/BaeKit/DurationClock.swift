import Foundation

/// Renders a millisecond duration as a "M:SS" clock label (e.g. "3:07") for the
/// current locale. bae-core emits the raw `duration_ms`; the UI formats it.
public enum DurationClock {
    /// "M:SS" for a duration in ms (e.g. "3:07"); "" when absent or negative.
    public static func text(_ ms: Int64?) -> String {
        guard let ms, ms >= 0 else { return "" }
        return Duration.milliseconds(ms)
            .formatted(.time(pattern: .minuteSecond(padMinuteToLength: 1)))
    }

    /// "-M:SS" remaining clock label from a position within a duration (e.g.
    /// "-1:23"). Both inputs are the pregap-adjusted track time core emits, so
    /// elapsed is just `position` and remaining is `duration − position`,
    /// clamped at zero.
    public static func remaining(positionMs: UInt64, durationMs: UInt64)
        -> String
    {
        let left = durationMs > positionMs ? durationMs - positionMs : 0
        return "-" + text(Int64(left))
    }
}
