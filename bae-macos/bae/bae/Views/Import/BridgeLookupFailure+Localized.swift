import BaeKit
import Foundation
import os.log

private let logger = Logger.bae("BridgeLookupFailure")

/// Resolve a `core.*` catalog key through the app's String Catalog.
private func localizedCore(_ key: String) -> String {
    NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
}

extension BridgeLookupFailure {
    /// The localized one-line reason. `provider` formats the HTTP status into
    /// the message (or a no-status fallback when absent); `diagnostic` has no
    /// translated copy, so it renders a generic line — the opaque detail is
    /// shown separately (`diagnosticDetail`), never as primary copy.
    var localizedDescription: String {
        switch self {
        case .diagnostic:
            // No translated copy — the opaque detail is shown separately.
            return localizedCore("core.lookup.failure.diagnostic")
        case .network, .timeout, .provider, .artworkAnalysis:
            // Core owns the key for every typed variant (including the
            // status-vs-no-status split for `provider`); the UI never picks it.
            guard let key = bridgeLookupFailureKey(failure: self) else {
                logger.warning(
                    "no catalog key for lookup failure \(String(reflecting: self))"
                )
                return localizedCore("core.lookup.failure.diagnostic")
            }
            let format = localizedCore(key)
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
