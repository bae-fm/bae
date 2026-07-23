import Foundation

/// Renders a duration as a clock label — "3:07", "1:12:34", "-0:42" — for the
/// current locale.
///
/// Core decides the label's fields: whether it has an hours field, whether it
/// exists at all, and how a countdown behaves at the end of a track
/// (`bridgeClock` / `bridgeSeekBar`). This renders those fields and
/// nothing else, so a given duration reads the same on every platform. The
/// digits are the locale's — "٣:٠٧" where an "en" locale reads "3:07" — which is
/// why the rendering is here and not in core.
public enum DurationClock {
    /// The clock label for a duration in ms; "" when there is nothing to label
    /// (no duration, or a negative one). For a value known only as milliseconds
    /// at render time (an outbox ETA); a static row instead carries its clock as
    /// a `BridgeDurationClock` field and renders it through ``label(_:)``.
    public static func text(_ ms: Int64?) -> String {
        render(bridgeClock(ms: ms))
    }

    /// The clock label for a pre-computed clock — the fields core rendered onto
    /// a row at conversion; "" when there is nothing to label.
    public static func label(_ clock: BridgeDurationClock?) -> String {
        render(clock)
    }

    /// The seek bar's two labels: the leading one shows the elapsed position or
    /// the countdown, per `showRemaining` (the user's config), and the trailing
    /// one is the track's total length — "" when its length is unknown. Which
    /// label is which is core's decision, not the bar's.
    public static func seekBar(
        positionMs: UInt64,
        durationMs: UInt64,
        showRemaining: Bool
    ) -> (leading: String, trailing: String) {
        let clocks = bridgeSeekBar(
            positionMs: positionMs,
            durationMs: durationMs,
            showRemaining: showRemaining
        )
        return (render(clocks.leading), render(clocks.trailing))
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
