import BaeKit
import Foundation
import Testing

@testable import bae

/// What one row's menu offers: core's offers for that row, under its key.
@Suite("The import row's action offers")
struct ImportCandidateActionOfferTests {
    /// Each offer core sends for the row is one offer for that row, in the
    /// order core sends them, with the number of rows it applies to and
    /// whether it can run, as core sends them.
    @Test("a row's menu offers what core offers for the row")
    func aRowsMenuOffersWhatCoreOffers() {
        let key = Data([1, 2, 3])
        let live = BridgeCandidateLiveState(
            import: nil,
            actions: [.revealFolder, .skip, .identify],
            needsYou: nil
        )
        let offers = live.offers.map {
            ImportCandidateActionOffer($0, rowKey: key)
        }

        #expect(offers.map(\.action) == [.revealFolder, .skip, .identify])
        #expect(
            offers.allSatisfy {
                $0.applicable == 1 && $0.enabled && $0.rowKey == key
            }
        )
    }
}
