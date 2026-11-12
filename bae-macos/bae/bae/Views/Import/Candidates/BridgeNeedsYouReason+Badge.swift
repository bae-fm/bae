import BaeKit
import Foundation

extension BridgeNeedsYouReason {
    /// The few words the row's badge says about why the folder waits on the
    /// person.
    var badgeLabel: String {
        switch self {
        case .matches(let count):
            String(localized: "\(Int(count)) matches")
        case .noTracklist:
            String(localized: "No tracklist")
        case .notFound:
            String(localized: "Not found")
        case .nothingToLookUp:
            String(localized: "Nothing to look up")
        }
    }
}

extension BridgeNeedsYouReason {
    /// The one plain sentence the Find online page opens with, saying what
    /// happened.
    var sentence: String {
        switch self {
        case .matches(let count):
            String(
                localized: "We found \(Int(count)) releases that could be this."
            )
        case .noTracklist:
            String(
                localized:
                    "The catalog doesn’t list this release’s tracks, so we couldn’t check it fits."
            )
        case .notFound:
            String(localized: "No catalog has this.")
        case .nothingToLookUp:
            String(
                localized:
                    "Your folder has no Disc ID, barcode, catalog number or names to search with."
            )
        }
    }
}

extension BridgeNeedsYouReason {
    /// Whether the lookup offered releases to pick from, which decides how
    /// keeping the folder's own draft is worded: none of these, or keep mine.
    var offeredReleases: Bool {
        switch self {
        case .matches, .noTracklist:
            true
        case .notFound, .nothingToLookUp:
            false
        }
    }
}

extension BridgePendingStanding {
    /// Why the lookup left the folder's answer to the person, for a row it
    /// did.
    var needsYou: BridgeNeedsYouReason? {
        guard case .needsYou(let reason) = self else { return nil }
        return reason
    }
}
