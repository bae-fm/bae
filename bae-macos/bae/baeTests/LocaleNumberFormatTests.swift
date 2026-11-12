import BaeKit
import Foundation
import Testing

@testable import bae

/// Clock labels and years follow the locale's digits, separators and sign.
@Suite("Locale number formats")
struct LocaleNumberFormatTests {
    private static func clock(
        negative: Bool = false,
        hours: UInt64? = nil,
        minutes: UInt32,
        seconds: UInt32
    ) -> BridgeDurationClock {
        BridgeDurationClock(
            negative: negative,
            hours: hours,
            minutes: minutes,
            seconds: seconds
        )
    }

    @Test(
        "a clock reads minutes:seconds, hours when core gave them",
        arguments: ["en", "fr", "de", "ja"]
    )
    func latinClocks(identifier: String) {
        let locale = Locale(identifier: identifier)
        #expect(
            DurationClock.render(
                Self.clock(minutes: 3, seconds: 7),
                locale: locale
            )
                == "3:07"
        )
        #expect(
            DurationClock.render(
                Self.clock(hours: 1, minutes: 12, seconds: 34),
                locale: locale
            ) == "1:12:34"
        )
        #expect(
            DurationClock.render(
                Self.clock(hours: 100, minutes: 0, seconds: 0),
                locale: locale
            ) == "100:00:00"
        )
    }

    @Test("a countdown carries the locale's minus")
    func countdownSign() {
        let countdown = Self.clock(negative: true, minutes: 0, seconds: 42)
        #expect(
            DurationClock.render(countdown, locale: Locale(identifier: "de"))
                == "-0:42"
        )
        // Arabic and Hebrew put a left-to-right mark before the minus so it
        // stays on the left of the digits.
        let arabic = DurationClock.render(
            countdown,
            locale: Locale(identifier: "ar")
        )
        #expect(arabic.hasSuffix("-0:42"))
        #expect(arabic.hasPrefix("\u{200E}"))
    }

    @Test("a clock uses the locale's digits")
    func localeDigits() {
        let persian = DurationClock.render(
            Self.clock(minutes: 3, seconds: 7),
            locale: Locale(identifier: "fa")
        )
        #expect(persian == "۳:۰۷")
    }

    @Test("a year has no thousands separator, in any locale")
    func yearsAreNotGrouped() {
        for identifier in ["en", "fr", "de", "ar", "pl"] {
            let year = PressingText.year(
                1973,
                locale: Locale(identifier: identifier)
            )
            #expect(!year.contains(","), "\(identifier): \(year)")
            #expect(!year.contains("\u{00A0}"), "\(identifier): \(year)")
            #expect(!year.contains("."), "\(identifier): \(year)")
            #expect(year.count == 4, "\(identifier): \(year)")
        }
        #expect(
            PressingText.year(1973, locale: Locale(identifier: "fa"))
                == "۱۹۷۳"
        )
    }

    @Test("a count in French groups with a narrow space")
    func countsFollowTheLocale() {
        let french = 12_345
            .formatted(
                .number.locale(Locale(identifier: "fr"))
            )
        #expect(french.contains("\u{202F}") || french.contains("\u{00A0}"))
        let german = 12_345
            .formatted(
                .number.locale(Locale(identifier: "de"))
            )
        #expect(german == "12.345")
    }

    @Test("the bit rate unit is translated where the language has its own")
    func bitrateUnitsAreTranslated() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: "bae/Localizable.xcstrings")
        let catalog = try #require(
            try JSONSerialization.jsonObject(with: Data(contentsOf: url))
                as? [String: Any]
        )
        let strings = try #require(catalog["strings"] as? [String: Any])
        let entry = try #require(strings["%lld kbps"] as? [String: Any])
        let localizations = try #require(
            entry["localizations"] as? [String: [String: [String: String]]]
        )
        func value(_ locale: String) -> String? {
            localizations[locale]?["stringUnit"]?["value"]
        }
        #expect(value("fr") == "%lld kbit/s")
        #expect(value("pl") == "%lld kb/s")
        #expect(value("ar") == "%lld كيلوبت/ث")
        #expect(value("uk") == "%lld кбіт/с")
    }
}
