import BaeKit
import Foundation

extension BridgeNeedsYouReason {
    /// The few words the row's badge says about why the folder waits on the
    /// person.
    var badgeLabel: String {
        switch self {
        case .matches(let count):
            String(localized: "\(Int(count)) matches")
        case .trackCountMismatch:
            String(localized: "Track count mismatch")
        case .noTracklist:
            String(localized: "No tracklist")
        case .mediumMismatch:
            String(localized: "Medium mismatch")
        case .notFound:
            String(localized: "Not found")
        case .nothingToLookUp:
            String(localized: "Nothing to look up")
        }
    }
}

extension BridgePendingStanding {
    /// The badge a row in this state wears: only a row waiting on the person
    /// has one, saying why.
    var badge: String? {
        guard case .needsYou(let reason) = self else { return nil }
        return reason.badgeLabel
    }
}
