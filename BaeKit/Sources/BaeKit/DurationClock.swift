import Foundation

/// Renders a duration as a clock label — "3:07", "1:12:34", "-0:42" — for the
/// current locale.
///
/// Core decides the label's fields: whether it has an hours field, whether it
/// exists at all, and how a countdown behaves at the end of a track
/// (`bridgeClock` / `bridgeRemainingClock`). This renders those fields and
/// nothing else, so a given duration reads the same on every platform. The
/// digits are the locale's — "٣:٠٧" where an "en" locale reads "3:07" — which is
/// why the rendering is here and not in core.
public enum DurationClock {
    /// The clock label for a duration in ms; "" when there is nothing to label
    /// (no duration, or a negative one).
    public static func text(_ ms: Int64?) -> String {
        render(bridgeClock(ms: ms))
    }

    /// The remaining clock label from a position within a duration (e.g.
    /// "-1:23"). Both inputs are the pregap-adjusted track time core emits.
    public static func remaining(positionMs: UInt64, durationMs: UInt64)
        -> String
    {
        render(
            bridgeRemainingClock(positionMs: positionMs, durationMs: durationMs)
        )
    }

    /// `:` between the fields, every field after the first padded to two digits,
    /// a leading `-` for a countdown.
    private static func render(_ clock: BridgeDurationClock?) -> String {
        guard let clock else { return "" }
        let sign = clock.negative ? "-" : ""
        let seconds = padded(UInt64(clock.seconds))
        guard let hours = clock.hours else {
            return "\(sign)\(plain(UInt64(clock.minutes))):\(seconds)"
        }
        let minutes = padded(UInt64(clock.minutes))
        return "\(sign)\(plain(hours)):\(minutes):\(seconds)"
    }

    /// The leading field: unpadded, and never grouped ("100:00", not "1,00:00").
    private static func plain(_ value: UInt64) -> String {
        value.formatted(.number.grouping(.never))
    }

    /// A trailing field: two digits, so 7 seconds reads "07". Only ever applied
    /// to minutes and seconds, which core keeps within 0...59.
    private static func padded(_ value: UInt64) -> String {
        value.formatted(.number.precision(.integerLength(2)).grouping(.never))
    }
}
