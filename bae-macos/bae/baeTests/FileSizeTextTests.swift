import BaeKit
import Foundation
import Testing

/// The size rule the boards print: decimal units, at most one decimal, none
/// when the rounded number is whole, the separator from the locale.
@Suite("File size text")
struct FileSizeTextTests {
    private let en = Locale(identifier: "en_US")

    /// The size with any no-break space between number and unit as a plain one.
    private func text(_ bytes: UInt64, _ locale: Locale) -> String {
        bytes.fileSizeText(locale: locale)
            .replacingOccurrences(of: "\u{202F}", with: " ")
            .replacingOccurrences(of: "\u{00A0}", with: " ")
    }

    @Test(
        "prints the boards' sizes",
        arguments: [
            (UInt64(3_860_000_000), "3.9 GB"),
            (1_200_000_000, "1.2 GB"),
            (420_000_000, "420 MB"),
            (1_000_000_000, "1 GB"),
            (20_000_000_000, "20 GB"),
            (6_400_000, "6.4 MB"),
            (6_000, "6 KB"),
            (0, "0 bytes"),
            (1, "1 byte"),
            (999, "999 bytes"),
            (1_000, "1 KB"),
        ]
    )
    func boardsSizes(bytes: UInt64, text expected: String) {
        #expect(text(bytes, en) == expected)
    }

    @Test("a value that rounds to a thousand moves up a unit")
    func roundsUpAUnit() {
        #expect(text(999_900, en) == "999.9 KB")
        #expect(text(999_960, en) == "1 MB")
        #expect(text(999_960_000_000, en) == "1 TB")
    }

    @Test("the largest unit is terabytes")
    func largestUnit() {
        #expect(
            text(12_000_000_000_000_000, en) == "12,000 TB"
        )
    }

    @Test("the decimal separator comes from the locale")
    func localeSeparator() {
        #expect(text(3_860_000_000, Locale(identifier: "de_DE")) == "3,9 GB")
    }

    @Test("the unit names come from the locale")
    func localeUnits() {
        let fr = Locale(identifier: "fr_FR")
        #expect(text(3_860_000_000, fr) == "3,9 Go")
        #expect(text(420_000_000, fr) == "420 Mo")
        #expect(text(3_860_000_000, Locale(identifier: "uk_UA")) == "3,9 ГБ")
        #expect(text(1, fr) == "1 o")
    }
}
