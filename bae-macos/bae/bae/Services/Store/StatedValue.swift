import BaeKit
import Foundation

/// One value the records of a pressing state for a fact, worded, and the
/// catalogs that state it. Core names the catalogs only where the records
/// state the fact differently, so a value no catalog is named for is shown
/// as stated.
struct StatedValue: Equatable {
    let value: String
    /// The catalogs' names as one list ("MusicBrainz and Discogs"); empty
    /// where core names none.
    let catalogs: String

    init(value: String, catalogs: [BridgeLookupCatalog]) {
        self.value = value
        self.catalogs = ListFormatter.localizedString(
            byJoining: catalogs.map { bridgeLookupCatalogName(catalog: $0) }
        )
    }

    /// The value followed by its catalogs: "1992 MusicBrainz".
    var text: String {
        catalogs.isEmpty ? value : "\(value) \(catalogs)"
    }
}

extension BridgeYearStatement {
    var stated: StatedValue {
        StatedValue(value: PressingText.year(year), catalogs: catalogs)
    }
}

extension BridgeTextStatement {
    var stated: StatedValue {
        StatedValue(value: text, catalogs: catalogs)
    }
}

extension BridgeTermsStatement {
    var stated: StatedValue {
        StatedValue(value: PressingText.line(terms), catalogs: catalogs)
    }
}

extension PressingText {
    /// The values one fact has, each as stated, joined by the catalog's
    /// separator for alternatives: an album's titles, "Low Tide Hours / Low
    /// Tide Hours (Remastered)".
    static func alternatives(_ values: [String]) -> String {
        values.joined(
            separator: localizedCoreString(
                "core.pressing.alternatives_separator"
            )
        )
    }

    /// The values of one fact, each followed by its catalogs, joined by the
    /// catalog's separator for alternatives: "1992 MusicBrainz / 1993
    /// Discogs".
    static func alternatives(_ values: [StatedValue]) -> String {
        values.map(\.text)
            .joined(
                separator: localizedCoreString(
                    "core.pressing.alternatives_separator"
                )
            )
    }
}
