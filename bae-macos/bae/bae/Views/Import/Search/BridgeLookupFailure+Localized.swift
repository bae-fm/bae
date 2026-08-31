import BaeKit
import Foundation
import os.log

private let logger = Logger.bae("BridgeLookupFailure")

extension BridgeLookupFailure {
    /// The localized one-line reason. `provider` formats the HTTP status into
    /// the message (or a no-status fallback when absent); `diagnostic` has no
    /// translated copy, so it renders a generic line — the opaque detail is
    /// shown separately (`diagnosticDetail`), never as primary copy.
    var localizedDescription: String {
        switch self {
        case .diagnostic:
            // No translated copy — the opaque detail is shown separately.
            return coreString("core.lookup.failure.diagnostic")
        case .network, .timeout, .provider, .artworkAnalysis:
            // Core owns the key for every typed variant (including the
            // status-vs-no-status split for `provider`); the UI never picks it.
            guard let key = bridgeLookupFailureKey(failure: self) else {
                logger.warning(
                    "no catalog key for lookup failure \(String(reflecting: self))"
                )
                return coreString("core.lookup.failure.diagnostic")
            }
            let format = coreString(key)
            if case .provider(let status) = self, let status {
                return String(format: format, NSNumber(value: status).intValue)
            }
            return format
        }
    }

    /// The opaque, log-only detail for a `diagnostic` failure — shown as
    /// secondary text, never translated. `nil` for the typed variants.
    var diagnosticDetail: String? {
        if case .diagnostic(let detail) = self {
            return detail
        }
        return nil
    }

    /// The full badge line: the localized reason, with the opaque diagnostic
    /// detail appended (so a local failure still surfaces its cause, never as
    /// primary copy). The typed variants render their reason alone.
    var badgeLine: String {
        if let detail = diagnosticDetail {
            return "\(localizedDescription): \(detail)"
        }
        return localizedDescription
    }
}

extension BridgeIdentifyFailure {
    /// The failed step plus its provider reason. Keeping the step attached is
    /// what distinguishes two simultaneous provider failures with the same
    /// underlying reason.
    var badgeLine: String {
        switch self {
        case .discId(let failure):
            return String(localized: "Disc ID") + ": " + failure.badgeLine
        case .barcode(let failure):
            return String(localized: "Barcode") + ": " + failure.badgeLine
        case .catalog(let failure):
            return String(localized: "Catalog number") + ": "
                + failure.badgeLine
        case .releaseDetails(let failure):
            return String(
                localized:
                    "Failed to load release details: \(failure.badgeLine)"
            )
        }
    }
}
