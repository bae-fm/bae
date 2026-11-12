import Foundation
import Testing

/// The count keys carry each locale's CLDR plural categories, so a count of 1
/// (and 2-4 in the Slavic locales, the six Arabic forms) reads right.
@Suite("Catalog plural forms")
struct CatalogPluralFormsTests {
    /// CLDR cardinal categories that integer counts reach, per catalog locale.
    /// `many` for es/fr/it/pt is the million-and-up form ("1 de millón").
    private static let categories: [String: Set<String>] = [
        "en": ["one", "other"],
        "ar": ["zero", "one", "two", "few", "many", "other"],
        "bg": ["one", "other"],
        "cs": ["one", "few", "many", "other"],
        "da": ["one", "other"],
        "de": ["one", "other"],
        "el": ["one", "other"],
        "es": ["one", "many", "other"],
        "fi": ["one", "other"],
        "fr": ["one", "many", "other"],
        "he": ["one", "two", "other"],
        "hr": ["one", "few", "other"],
        "hu": ["one", "other"],
        "it": ["one", "many", "other"],
        "ja": ["other"],
        "ko": ["other"],
        "nb": ["one", "other"],
        "nl": ["one", "other"],
        "pl": ["one", "few", "many", "other"],
        "pt-BR": ["one", "many", "other"],
        "pt-PT": ["one", "many", "other"],
        "ro": ["one", "few", "other"],
        "sk": ["one", "few", "many", "other"],
        "sv": ["one", "other"],
        "tr": ["one", "other"],
        "uk": ["one", "few", "many", "other"],
        "zh-Hans": ["other"],
        "zh-Hant": ["other"],
    ]

    private static let countKeys = [
        "%lld releases",
        "%lld tracks",
        "%lld more releases",
        "%llu files · %@",
        "%llu files · %@ · %@",
    ]

    private static func strings() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: "bae/Localizable.xcstrings")
        let catalog = try #require(
            try JSONSerialization.jsonObject(with: Data(contentsOf: url))
                as? [String: Any]
        )
        return try #require(catalog["strings"] as? [String: Any])
    }

    private static func forms(
        _ key: String,
        _ locale: String
    ) throws -> [String: String] {
        let entry = try #require(try strings()[key] as? [String: Any])
        let localizations = try #require(
            entry["localizations"] as? [String: Any]
        )
        let localization = try #require(
            localizations[locale] as? [String: Any],
            "\(key) has no \(locale)"
        )
        let plural = try #require(
            (localization["variations"] as? [String: Any])?["plural"]
                as? [String: [String: [String: String]]],
            "\(key) in \(locale) has no plural variations"
        )
        return plural.mapValues { $0["stringUnit"]?["value"] ?? "" }
    }

    @Test(
        "each count key has the locale's plural categories",
        arguments: countKeys
    )
    func everyLocaleHasItsCategories(key: String) throws {
        for (locale, expected) in Self.categories {
            let forms = try Self.forms(key, locale)
            #expect(
                Set(forms.keys) == expected,
                "\(key) in \(locale): \(Set(forms.keys)) != \(expected)"
            )
            #expect(!forms.values.contains(""), "\(key) in \(locale)")
        }
    }

    @Test("English words one as singular")
    func englishSingular() throws {
        #expect(try Self.forms("%lld releases", "en")["one"] == "%lld release")
        #expect(try Self.forms("%lld tracks", "en")["one"] == "%lld track")
        #expect(
            try Self.forms("%llu files · %@ · %@", "en")["one"]
                == "%1$llu file · %2$@ · %3$@"
        )
    }

    @Test("Polish and Arabic tell 1, 2-4 and 5+ apart")
    func slavicAndArabicForms() throws {
        let pl = try Self.forms("%lld tracks", "pl")
        #expect(pl["one"] != pl["few"])
        #expect(pl["few"] != pl["many"])
        let ar = try Self.forms("%lld releases", "ar")
        #expect(Set(ar.values).count >= 5)
    }
}
