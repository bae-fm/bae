import BaeKit
import Foundation

extension BridgePendingFilter {
    var label: String {
        switch self {
        case .all: String(localized: "All")
        case .needsYou: String(localized: "Needs You")
        case .inProgress: String(localized: "In Progress")
        case .identified: String(localized: "Identified")
        case .unmatched: String(localized: "Unmatched")
        case .notLookedUp: String(localized: "Not Looked Up")
        }
    }

    /// The name with how many rows the entry holds, for the menu item, where
    /// the number has no place of its own.
    func label(count: UInt32) -> String {
        String(localized: "\(label) (\(Int(count)))")
    }
}
