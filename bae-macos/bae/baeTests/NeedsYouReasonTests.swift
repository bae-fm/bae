import BaeKit
import Testing

@testable import bae

/// What a Found row waiting on the person says about why. Which reason it
/// is, and its numbers, are core's.
@Suite("Why a folder waits on the person")
struct NeedsYouReasonTests {
    @Test("only a row waiting on the person wears a badge")
    func onlyNeedsYouWearsABadge() {
        let waiting = BridgePendingStanding.needsYou(reason: .notFound)
        #expect(waiting.badge == String(localized: "Not found"))
        let others: [BridgePendingStanding] = [
            .notLookedUp, .identifying, .identified, .unmatched,
            .lookupError, .error(failure: BridgeInternalFailure(detail: "x")),
            .importing, .importError,
        ]
        for standing in others {
            #expect(standing.badge == nil, "\(standing)")
        }
    }

    @Test("each reason has its own badge")
    func eachReasonHasItsOwnBadge() {
        let reasons: [BridgeNeedsYouReason] = [
            .matches(count: 3),
            .trackCountMismatch(local: 11, source: 12),
            .noTracklist,
            .mediumMismatch(folder: .cdRip, releases: 2),
            .notFound,
            .nothingToLookUp,
        ]
        let badges = reasons.map(\.badgeLabel)
        #expect(Set(badges).count == reasons.count, "\(badges)")
        #expect(
            BridgeNeedsYouReason.matches(count: 3).badgeLabel
                == String(localized: "\(3) matches")
        )
    }
}
