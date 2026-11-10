import BaeKit
import SwiftUI

/// Presentation strings for a signal, all derived from the pre-shaped fields
/// core sends. Kept as a caseless namespace so the ledger's group labels and the
/// notes closing a result list word a signal the same way.
enum SignalBadgeStyle {
    /// One badge on a pressing row: what the candidate's own text agrees with
    /// about that release. The first three name the lookup that returned it,
    /// the rest name a field the folder's text states.
    enum Agreement {
        case discId
        case barcode
        case catalog
        case label
        case year
        case country
        case notes
    }

    /// The identifying signals, as a failure line or the
    /// run's lookup row name them.
    enum Signal {
        case discId
        case barcode
        case catalog
        case isrc
    }

    /// The agreement's name on its own — a badge.
    static func label(for agreement: Agreement) -> String {
        switch agreement {
        case .discId: label(for: Signal.discId)
        case .barcode: label(for: Signal.barcode)
        case .catalog: label(for: Signal.catalog)
        case .label: String(localized: "Label")
        case .year: String(localized: "Year")
        case .country: String(localized: "Country")
        case .notes: String(localized: "Notes")
        }
    }

    /// The signal's name on its own — a row label.
    static func label(for kind: Signal) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "Barcode")
        case .catalog: String(localized: "Catalog")
        case .isrc: String(localized: "ISRC")
        }
    }

    /// The signal's name inside a sentence — "Discogs barcode results are
    /// missing from this list." A proper noun keeps its capitals; the rest
    /// are common nouns.
    static func sentenceLabel(for kind: Signal) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "barcode")
        case .catalog: String(localized: "catalog number")
        case .isrc: String(localized: "ISRC")
        }
    }

    /// The same, for a step a provider was asked: the identifiers, or the
    /// title the run searched by.
    static func sentenceLabel(for step: FailedSearch.Step) -> String {
        switch step {
        case .signal(let kind): sentenceLabel(for: kind)
        case .titleSearch: String(localized: "title search")
        case .pressing: String(localized: "pressing lookup")
        }
    }
}
