import BaeKit
import Testing

@testable import bae

/// What a Found row waiting on the person says about why. Which reason it
/// is, and its numbers, are core's.
@Suite("Why a folder waits on the person")
struct NeedsYouReasonTests {
    /// Which rows wear a badge, and its tone, are core's; each badge says
    /// its own words in the tone core gave it.
    @Test("each badge core gives says its own words")
    func eachBadgeSaysItsOwnWords() {
        let badges: [BridgePendingBadge] = [
            .needsYou(reason: .notFound), .lookupError, .error, .importError,
        ]
        let labels = badges.map(\.label)
        #expect(Set(labels).count == badges.count, "\(labels)")
        #expect(
            BridgePendingBadge.needsYou(reason: .notFound).label
                == String(localized: "Not found")
        )
        #expect(
            BridgePendingBadge.lookupError.label
                == String(localized: "Lookup error")
        )
        #expect(BridgeBadgeTone.attention.statusTone == .warning)
        #expect(BridgeBadgeTone.failure.statusTone == .danger)
    }

    @Test("each reason has its own badge")
    func eachReasonHasItsOwnBadge() {
        let reasons: [BridgeNeedsYouReason] = [
            .matches(count: 3),
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

    @Test("the sentence carries the numbers core gave it")
    func theSentenceCarriesTheNumbers() {
        #expect(
            BridgeNeedsYouReason.matches(count: 4).sentence
                == String(
                    localized: "We found \(4) releases that could be this."
                )
        )
    }

    @Test("a medium that rules out one release reads in the singular")
    func mediumSentenceFollowsTheCount() {
        let one = BridgeNeedsYouReason.mediumMismatch(
            folder: .cdRip,
            releases: 1
        )
        let two = BridgeNeedsYouReason.mediumMismatch(
            folder: .cdRip,
            releases: 2
        )
        #expect(one.sentence != two.sentence)
        #expect(
            BridgeNeedsYouReason.mediumMismatch(
                folder: .notCdAudio,
                releases: 2
            )
            .sentence != two.sentence
        )
    }

    /// Keeping the folder's own draft reads "None of these" where the page
    /// offered releases, and "Keep my info" where it offered none.
    @Test("only a page that offered releases says none of these")
    func offeredReleasesDecideTheWording() {
        let offered: [BridgeNeedsYouReason] = [
            .matches(count: 2),
            .noTracklist,
            .mediumMismatch(folder: .notCdAudio, releases: 1),
        ]
        for reason in offered {
            #expect(reason.offeredReleases, "\(reason)")
        }
        #expect(!BridgeNeedsYouReason.notFound.offeredReleases)
        #expect(!BridgeNeedsYouReason.nothingToLookUp.offeredReleases)
    }
}
