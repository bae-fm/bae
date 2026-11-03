import BaeKit
import Foundation
import Testing

@testable import bae

/// Several pressings of one album offer the way to answer without saying
/// which, with what it does; a list core says is not that offers none.
@MainActor
@Suite("Not sure")
struct NotSureOfferTests {
    /// Several pressings of one album offered to a folder waiting on the
    /// person, which it can be linked to with its pressing unknown.
    private static let sharedAlbumState = PreviewData.searchState(
        identifyState: .found(
            run: PreviewData.identifyRunFound,
            groups: [PreviewData.searchGroupExact],
            trackCount: 11,
            agreements: PreviewData.searchAgreementsExact,
            narrowedOutCount: 0,
            folderCheck: nil,
            unattendedPick: nil,
            offersSharedAlbum: true
        ),
        signals: PreviewData.settledSignals,
        needsYou: .matches(count: 2)
    )

    /// Core says whether the rows are pressings of one album; the pane
    /// offers Not sure on that alone. Its words are the
    /// `import-search-not-sure` shot's.
    @Test("several pressings of one album offer Not sure; others do not")
    func severalPressingsOfOneAlbumOfferNotSure() {
        #expect(Self.sharedAlbumState.offersSharedAlbum)
        let notOffered = PreviewData.searchState(
            identifyState: IdentifyState(
                bridge: PreviewData.bridgeDisagreementState
            ),
            needsYou: .matches(count: 2)
        )
        #expect(!notOffered.offersSharedAlbum)
    }
}
