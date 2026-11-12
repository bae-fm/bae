import Foundation

extension BinaryInteger {
    /// A byte count as the boards print it, in the person's language: "3.9 GB",
    /// "420 MB", "1 GB".
    public var fileSizeText: String {
        fileSizeText(locale: .current)
    }

    /// A byte count in decimal units (1 KB is 1000 bytes), the largest unit
    /// that keeps the number at 1 or more, with at most one decimal and none
    /// when the rounded number is whole. The number takes `locale`'s separator
    /// and digits and goes into the catalog's text for the unit in `locale`'s
    /// language. A value that rounds up to 1000 moves to the next unit, so
    /// 999,960 bytes is "1 MB".
    public func fileSizeText(locale: Locale) -> String {
        let bytes = UInt64(clamping: self)
        var unit = 0
        var divisor = 1.0
        // Tenths of the current unit, rounded half up.
        func tenths() -> Double {
            (Double(bytes) * 10 / divisor + 0.5).rounded(.down)
        }
        while unit < 4, tenths() >= 10_000 {
            divisor *= 1000
            unit += 1
        }
        let value = tenths() / 10
        if unit == 0 {
            return String(
                localized: LocalizedStringResource(
                    "\(Int(value)) bytes",
                    locale: locale
                )
            )
        }
        let number = value.formatted(
            .number.precision(.fractionLength(0...1)).locale(locale)
        )
        let text: LocalizedStringResource =
            switch unit {
            case 1: LocalizedStringResource("\(number) KB", locale: locale)
            case 2: LocalizedStringResource("\(number) MB", locale: locale)
            case 3: LocalizedStringResource("\(number) GB", locale: locale)
            default: LocalizedStringResource("\(number) TB", locale: locale)
            }
        return String(localized: text)
    }
}
