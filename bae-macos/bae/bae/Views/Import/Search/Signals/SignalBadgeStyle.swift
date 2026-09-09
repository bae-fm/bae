import BaeKit
import SwiftUI

/// Presentation strings for a signal, all derived from the pre-shaped fields
/// core sends. Kept as a caseless namespace so the ledger's group labels, the
/// evidence chips on a candidate's files, and the notes closing a result
/// list word a signal the same way.
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
    }

    /// The agreement's name on its own — a badge.
    static func label(for agreement: Agreement) -> String {
        switch agreement {
        case .discId: label(for: BridgeSignalKind.discId)
        case .barcode: label(for: BridgeSignalKind.barcode)
        case .catalog: label(for: BridgeSignalKind.catalog)
        case .label: String(localized: "Label")
        case .year: String(localized: "Year")
        case .country: String(localized: "Country")
        }
    }

    static func icon(for kind: BridgeSignalKind) -> String {
        switch kind {
        case .discId: "opticaldiscdrive"
        case .barcode: "barcode"
        case .catalog: "tag"
        }
    }

    /// The signal's name on its own — a row label.
    static func label(for kind: BridgeSignalKind) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "Barcode")
        case .catalog: String(localized: "Catalog")
        }
    }

    /// The signal's name inside a sentence — "Discogs barcode results are
    /// missing from this list." A proper noun keeps its capitals; the rest
    /// are common nouns.
    static func sentenceLabel(for kind: BridgeSignalKind) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "barcode")
        case .catalog: String(localized: "catalog number")
        }
    }

    /// Where a value was read: what a source chip says on hover when it has
    /// no file to name.
    static func originLabel(for origin: BridgeSignalOrigin) -> String {
        switch origin {
        case .discToc: String(localized: "Disc TOC")
        case .cueSheet: String(localized: "CUE sheet")
        case .artwork: String(localized: "Cover OCR")
        case .folderName: String(localized: "folder name")
        case .filename: String(localized: "file name")
        case .textFile: String(localized: "Text file")
        }
    }
}
