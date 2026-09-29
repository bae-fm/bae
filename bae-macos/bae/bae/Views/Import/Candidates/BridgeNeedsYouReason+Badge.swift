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
        case .mediumMismatch:
            String(localized: "Medium mismatch")
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
                    "The catalog doesn't list this release's tracks, so we couldn't check it fits."
            )
        case .mediumMismatch(.cdRip, releases: 1):
            String(
                localized:
                    "Your files look like a CD rip, but the release found isn't a CD."
            )
        case .mediumMismatch(.cdRip, _):
            String(
                localized:
                    "Your files look like a CD rip, but the releases found aren't CDs."
            )
        case .mediumMismatch(.notCdAudio, releases: 1):
            String(
                localized:
                    "Your files have a sample rate no CD has, but the release found is a CD."
            )
        case .mediumMismatch(.notCdAudio, _):
            String(
                localized:
                    "Your files have a sample rate no CD has, but the releases found are CDs."
            )
        case .notFound:
            String(localized: "No catalog has this.")
        case .nothingToLookUp:
            String(
                localized:
                    "Your folder has no disc ID, barcode, catalog number or names to search with."
            )
        }
    }
}

extension BridgeNeedsYouReason {
    /// Whether the lookup offered releases to pick from, which decides how
    /// keeping the folder's own draft is worded: none of these, or keep mine.
    var offeredReleases: Bool {
        switch self {
        case .matches, .noTracklist, .mediumMismatch:
            true
        case .notFound, .nothingToLookUp:
            false
        }
    }
}

extension BridgePendingStanding {
    /// Why the folder waits on the person, for a row that does.
    var needsYou: BridgeNeedsYouReason? {
        guard case .needsYou(let reason) = self else { return nil }
        return reason
    }

    /// The badge a row in this state wears: only a row waiting on the person
    /// has one, saying why.
    var badge: String? {
        needsYou?.badgeLabel
    }
}
