import BaeKit
import SwiftUI

/// Presentation strings for a signal, all derived from the pre-shaped fields
/// core sends. Kept as a caseless namespace so the header's verdict line and
/// the Adjust popover word a signal the same way.
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

    /// The signal's name inside a sentence — "Identified by Disc ID and
    /// barcode". A proper noun keeps its capitals; the rest are common nouns.
    static func sentenceLabel(for kind: BridgeSignalKind) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "barcode")
        case .catalog: String(localized: "catalog number")
        }
    }

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

    /// Whether the signal is waiting to be told which of its values to use —
    /// the catalog, before one of the extracted numbers is checked. It is not
    /// in the run, and its count is how many there are to choose from.
    static func awaitingChoice(_ signal: BridgeToolbarSignal) -> Bool {
        !signal.options.isEmpty && signal.value == nil
    }

    static func stateLabel(for signal: BridgeToolbarSignal) -> String {
        if signal.excluded {
            return String(localized: "Excluded from search")
        }
        if awaitingChoice(signal) {
            return String(localized: "Pick a catalog number to look up")
        }
        switch signal.state {
        case .lookingUp:
            return String(localized: "Looking up\u{2026}")
        case .found(let count):
            return String(localized: "\(Int(count)) releases")
        case .noMatch:
            return String(localized: "No releases matched")
        case .skipped:
            switch signal.kind {
            case .discId: return String(localized: "No disc layout")
            case .catalog: return String(localized: "No catalog number found")
            case .barcode: return String(localized: "No source to scan")
            }
        case .failed(let failure):
            return failure.badgeLine
        }
    }
}
