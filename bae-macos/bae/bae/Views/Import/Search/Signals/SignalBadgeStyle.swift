import BaeKit
import SwiftUI

/// Presentation strings for a signal, all derived from the pre-shaped fields
/// core sends. Kept as a caseless namespace so the ledger's group labels, the
/// evidence chips on a candidate's files, and the notes closing a result
/// list word a signal the same way.
enum SignalBadgeStyle {
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
