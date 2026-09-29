import BaeKit
import Foundation

extension BridgeLookupFailure {
    /// The localized one-line reason. Core owns the key for every variant,
    /// including the status-vs-no-status split for `provider`; this formats
    /// the HTTP status into it.
    var badgeLine: String {
        let format = coreString(bridgeLookupFailureKey(failure: self))
        if case .provider(let status) = self, let status {
            return String(format: format, NSNumber(value: status).intValue)
        }
        return format
    }
}

extension BridgeIdentifyFailure {
    /// The failed step plus its reason — and, where several providers answer
    /// one step, the provider that did not. Keeping both attached is what
    /// distinguishes two simultaneous failures with the same underlying reason.
    var badgeLine: String {
        switch self {
        case .discId(let failure):
            return String(localized: "Disc ID") + ": " + failure.badgeLine
        case .barcode(let source, let failure):
            return String(localized: "Barcode") + " \u{00b7} "
                + bridgeCatalogName(catalog: source) + ": "
                + failure.badgeLine
        case .catalog(let source, let failure):
            return String(localized: "Catalog number") + " \u{00b7} "
                + bridgeCatalogName(catalog: source) + ": "
                + failure.badgeLine
        case .search(let source, let failure):
            return String(localized: "Title") + " \u{00b7} "
                + bridgeCatalogName(catalog: source) + ": "
                + failure.badgeLine
        case .isrc(let failure):
            return String(localized: "ISRC") + ": " + failure.badgeLine
        case .releaseDetails(let failure):
            return String(
                localized:
                    "Failed to load release details: \(failure.badgeLine)"
            )
        }
    }
}
