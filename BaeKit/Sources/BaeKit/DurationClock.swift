import Foundation

/// Renders a duration as a clock label — "3:07", "1:12:34", "-0:42" — in the
/// current locale's format (`Duration.TimeFormatStyle`).
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
    /// at render time (a release's length in the metadata editor); a static
    /// row instead carries its clock as a `BridgeDurationClock` field and
    /// renders it through ``label(_:)``.
    public static func text(_ ms: Int64?) -> String {
        bridgeClock(ms: ms).map { render($0) } ?? ""
    }

    /// The clock label for a pre-computed clock — the fields core rendered onto
    /// a row at conversion (a track's length, which every track has).
    public static func label(_ clock: BridgeDurationClock) -> String {
        render(clock)
    }

    /// The seek bar's two labels: the leading one shows the pregap countdown
    /// before track start, then the elapsed position or end countdown per
    /// `showRemaining` (the user's config). The trailing one is the track's
    /// total length — "" when its length is unknown (`durationMs` nil). Which
    /// clock is shown is core's decision, not the bar's.
    public static func seekBar(
        positionMs: Int64,
        durationMs: UInt64?,
        showRemaining: Bool
    ) -> (leading: String, trailing: String) {
        let clocks = bridgeSeekBar(
            positionMs: positionMs,
            durationMs: durationMs,
            showRemaining: showRemaining
        )
        return (
            render(clocks.leading), clocks.trailing.map { render($0) } ?? ""
        )
    }

    /// The platform's clock format for the locale: its separator and digits,
    /// every field after the first padded to two digits, and its own sign for
    /// a countdown (a minus that a right-to-left run keeps on the left).
    public static func render(
        _ clock: BridgeDurationClock,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        let seconds =
            Int64(clock.minutes) * 60 + Int64(clock.seconds)
            + Int64(clamping: clock.hours ?? 0) * 3600
        let duration = Duration.seconds(clock.negative ? -seconds : seconds)
        let pattern: Duration.TimeFormatStyle.Pattern =
            clock.hours == nil
            ? .minuteSecond(padMinuteToLength: 1)
            : .hourMinuteSecond(padHourToLength: 1)
        return duration.formatted(.time(pattern: pattern).locale(locale))
    }
}
