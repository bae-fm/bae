import Foundation

/// What a pressing is, worded for the current locale. Core decides which
/// parts a line has and in what order, and a record carries them as terms;
/// this words each part and joins them, so a MusicBrainz row and a Discogs
/// row read the same shape: "Japan · 2×CD" over "Promo · Reissue".
public enum PressingText {
    /// Media the person is editing, each carrier with its count: "2×CD ·
    /// DVD". A draft's values are the form's own, not a record core words,
    /// so the terms are asked for here.
    public static func media(_ media: [BridgeMediaCount]) -> String {
        line(bridgeMediaTerms(media: media))
    }

    /// The parts, worded and joined with the catalog's list separator.
    public static func line(_ terms: [BridgeFactTerm]) -> String {
        terms.map(\.text)
            .joined(
                separator: QueueSummary.message("core.audio.list_separator")
            )
    }

    /// A country's name in the current locale, from its ISO 3166-1 code.
    public static func countryName(_ code: String) -> String {
        Locale.current.localizedString(forRegionCode: code) ?? code
    }
}

extension BridgeTermLabel {
    /// The value's word in the current locale, or its term as printed.
    public var text: String {
        switch self {
        case .localized(let key):
            QueueSummary.message(key)
        case .verbatim(let text):
            text
        }
    }
}

extension BridgeFactTerm {
    public var text: String {
        switch self {
        case .country(let code):
            PressingText.countryName(code)
        case .worded(let label):
            label.text
        case .counted(let count, let label):
            String.localizedStringWithFormat(
                QueueSummary.message("core.pressing.media_count"),
                Int(count),
                label.text
            )
        }
    }
}

extension BridgeReleaseArea {
    /// The area's name in the current locale.
    public var text: String {
        switch self {
        case .country(let code):
            PressingText.countryName(code)
        case .region(let region):
            QueueSummary.message(bridgeRegionKey(region: region))
        }
    }
}

extension BridgeReleaseName {
    /// What a list of an album's releases calls this one, in the current
    /// locale.
    public var text: String {
        switch self {
        case .named(let name):
            name
        case .described(let year, let media):
            ([year.map { String($0) }] + [PressingText.line(media)])
                .compactMap { $0 }
                .filter { !$0.isEmpty }
                .joined(separator: " ")
        case .numbered(let number):
            String.localizedStringWithFormat(
                QueueSummary.message("core.release.numbered"),
                Int(number)
            )
        }
    }
}

extension BridgeWorkReleaseSummary {
    /// The release's name, and its media where the name does not already say
    /// them — a release named by its year and media is not followed by them a
    /// second time.
    public var metadataText: String {
        switch name {
        case .described:
            name.text
        case .named, .numbered:
            ([name.text] + (media.isEmpty ? [] : [PressingText.line(media)]))
                .joined(
                    separator: QueueSummary.message("core.audio.list_separator")
                )
        }
    }
}
