import Foundation

extension BridgeDurationUnits {
    /// A total playing time in words — "39 min", "3 hr", "3 hr, 42 min" — for the
    /// current locale.
    ///
    /// Core decides which words the label has (an hour of music is "1 hr", never
    /// "1 hr, 0 min"); the `Core` catalog owns the words themselves and the
    /// pattern that joins them, so every platform says the same thing. The hours
    /// word is a plural — Hebrew's dual "שעתיים" drops the numeral entirely —
    /// which is why the two halves are formatted separately and then joined.
    public var text: String {
        switch self {
        case .hoursOnly(let hours):
            Self.hoursText(hours)
        case .minutesOnly(let minutes):
            Self.minutesText(minutes)
        case .hoursAndMinutes(let hours, let minutes):
            String(
                format: QueueSummary.message("core.duration.hours_minutes"),
                Self.hoursText(hours),
                Self.minutesText(minutes)
            )
        }
    }

    private static func hoursText(_ hours: UInt64) -> String {
        String.localizedStringWithFormat(
            QueueSummary.message("core.duration.hours"),
            Int(hours)
        )
    }

    private static func minutesText(_ minutes: UInt64) -> String {
        String.localizedStringWithFormat(
            QueueSummary.message("core.duration.minutes"),
            Int(minutes)
        )
    }
}
