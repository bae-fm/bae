import BaeKit
import Foundation
import Testing

@testable import bae

/// What one row's menu offers, read from the commands its live state lists.
@Suite("The import row's action offers")
struct ImportCandidateActionOfferTests {
    /// Each command the row lists is one offer for that row, in the order
    /// listed, with a count of one and nothing disabled; the combine command
    /// makes several rows one release, so a lone row's menu leaves it out.
    @Test("a row offers each command its live state lists except combining")
    func aRowOffersEachCommandItListsExceptCombining() {
        let key = Data([1, 2, 3])
        let offers = ImportCandidateActionOffer.row(
            key,
            actions: [.revealFolder, .skip, .combine, .identify, .import]
        )

        #expect(
            offers.map(\.action) == [.revealFolder, .skip, .identify, .import]
        )
        #expect(
            offers.allSatisfy {
                $0.applicable == 1 && $0.enabled && $0.rowKey == key
            }
        )
    }
}
