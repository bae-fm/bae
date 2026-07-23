import BaeKit
import SwiftUI

/// Presentation strings + colors for a badge, all derived from the badge's
/// pre-shaped fields. Kept as a caseless namespace so both the badge and its
/// popover render identically.
enum SignalBadgeStyle {
    static func icon(for kind: BridgeSignalKind) -> String {
        switch kind {
        case .discId: "opticaldiscdrive"
        case .barcode: "barcode"
        case .catalog: "tag"
        }
    }

    static func label(for kind: BridgeSignalKind) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "Barcode")
        case .catalog: String(localized: "Catalog")
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

    static func roleLabel(for role: BridgeSignalRole) -> String {
        switch role {
        case .identity: String(localized: "Identifies · finds releases")
        case .filter: String(localized: "Refines · narrows the match")
        }
    }

    static func stateLabel(for signal: BridgeToolbarSignal) -> String {
        if signal.excluded {
            return String(localized: "Excluded from search")
        }
        switch signal.state {
        case .lookingUp:
            return signal.role == .filter
                ? String(localized: "Matching\u{2026}")
                : String(localized: "Looking up\u{2026}")
        case .found(let count):
            return String(localized: "\(Int(count)) releases")
        case .noMatch:
            return String(localized: "No releases matched")
        case .skipped:
            return signal.kind == .discId
                ? String(localized: "No disc layout")
                : String(localized: "No source to scan")
        case .failed(let failure):
            return failure.badgeLine
        case .confirms(let count):
            return count > 0
                ? String(localized: "Matches this pressing")
                : String(localized: "No matched pressing carries this catno")
        }
    }

    static func stateDotColor(for signal: BridgeToolbarSignal) -> Color {
        if signal.excluded {
            return .secondary
        }
        switch signal.state {
        case .found(let count): return count > 0 ? .green : .orange
        case .confirms(let count): return count > 0 ? Theme.accent : .orange
        case .noMatch: return .orange
        case .failed: return .orange
        case .skipped: return .secondary
        case .lookingUp: return .secondary
        }
    }
}
