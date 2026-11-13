import BaeKit
import Foundation

extension BridgeLookupFailure {
    /// The localized one-line reason. Core owns the key for every variant,
    /// including the status-vs-no-status split for `provider`; this formats
    /// the HTTP status into it.
    var badgeLine: String {
        let format = localizedCoreString(bridgeLookupFailureKey(failure: self))
        if case .provider(let status) = self, let status {
            return String(format: format, NSNumber(value: status).intValue)
        }
        return format
    }

    /// The line a pressing row shows when its release document could not be
    /// read.
    var unreadDocumentLine: String {
        String(localized: "Failed to load release details: \(badgeLine)")
    }
}

extension BridgeIdentifyFailure {
    /// The failed step plus its reason — and, where several providers answer
    /// one step, the provider that did not. Keeping both attached is what
    /// distinguishes two simultaneous failures with the same underlying reason.
    var badgeLine: String {
        switch self {
        case .discId(let failure):
            return Self.line(String(localized: "Disc ID"), failure)
        case .barcode(let source, let failure):
            return Self.line(String(localized: "Barcode"), source, failure)
        case .catalog(let source, let failure):
            return Self.line(
                String(localized: "Catalog number"),
                source,
                failure
            )
        case .search(let source, let failure):
            return Self.line(String(localized: "Title"), source, failure)
        case .isrc(let failure):
            return Self.line(String(localized: "ISRC"), failure)
        case .pressing(let source, let failure):
            return Self.line(
                String(localized: "Same pressing"),
                source,
                failure
            )
        case .cover(let source, let failure):
            let catalog = bridgeCatalogName(catalog: source)
            return String(
                localized:
                    "Failed to load the cover from \(catalog): \(failure.badgeLine)"
            )
        }
    }

    /// "Step: reason" for a step with one possible answerer.
    private static func line(_ step: String, _ failure: BridgeLookupFailure)
        -> String
    {
        String(localized: "\(step): \(failure.badgeLine)")
    }

    /// "Step · Provider: reason" for a step several providers answer.
    private static func line(
        _ step: String,
        _ source: BridgeCatalog,
        _ failure: BridgeLookupFailure
    ) -> String {
        String(
            localized:
                "\(FactList.joined([step, bridgeCatalogName(catalog: source)])): \(failure.badgeLine)"
        )
    }
}
