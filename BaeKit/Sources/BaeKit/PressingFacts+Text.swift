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
        FactList.joined(terms.map(\.text))
    }

    /// A year in the current locale's digits, without a thousands separator
    /// ("1973", never "1,973").
    public static func year(
        _ year: some BinaryInteger,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        Int(year).formatted(.number.grouping(.never).locale(locale))
    }

    /// A country's name in the current locale, from its ISO 3166-1 code.
    public static func countryName(_ code: String) -> String {
        Locale.current.localizedString(forRegionCode: code) ?? code
    }

    /// The edition line: what is printed on the release. Its year when the
    /// line names one, each label with its catalog numbers, then where it was
    /// released, as in "2001 · Harbor Records · HR-1042 · US".
    public static func editionLine(
        year: String?,
        labels: [BridgeLabelLine],
        area: [BridgeFactTerm]
    ) -> String {
        FactList.joined([
            year,
            labelsText(labels),
            line(area),
        ])
    }

    /// The copy line: what the person has. The media and details, then the
    /// play time in the words core chose for it, as in "2×LP · Remastered ·
    /// 52 min".
    public static func copyLine(
        media: [BridgeFactTerm],
        details: [BridgeFactTerm],
        length: BridgeDurationUnits?
    ) -> String {
        FactList.joined([line(media), line(details), length?.text])
    }

    /// Each line core grouped: its names, then the catalog numbers they
    /// share, the lines one after another.
    static func labelsText(_ lines: [BridgeLabelLine]) -> String {
        let within = localizedCoreString("core.label.list_separator")
        return FactList.joined(
            lines.map { line in
                FactList.joined([
                    line.names.joined(separator: within),
                    line.catalogNumbers.joined(separator: within),
                ])
            }
        )
    }
}

extension BridgeTermLabel {
    /// The value's word in the current locale, or its term as printed.
    public var text: String {
        switch self {
        case .localized(let key):
            localizedCoreString(key)
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
                localizedCoreString("core.pressing.media_count"),
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
            localizedCoreString(bridgeRegionKey(region: region))
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
            Self.described(
                year: year.map { PressingText.year($0) },
                media: PressingText.line(media)
            )
        case .numbered(let number):
            String.localizedStringWithFormat(
                localizedCoreString("core.release.numbered"),
                Int(number)
            )
        }
    }

    /// The year and the media, in the order the locale writes them; either
    /// alone when the other is missing.
    private static func described(year: String?, media: String) -> String {
        switch (year, media.isEmpty) {
        case (let year?, false): String(localized: "\(year) \(media)")
        case (let year?, true): year
        case (nil, _): media
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
                    separator: localizedCoreString("core.audio.list_separator")
                )
        }
    }
}
