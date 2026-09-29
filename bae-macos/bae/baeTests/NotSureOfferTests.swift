import AppKit
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
            libraryStatuses: [:],
            trackCount: 11,
            agreements: PreviewData.searchAgreementsExact,
            narrowedOutCount: 0,
            catalogAgreements: PreviewData.catalogAgreements,
            folderCheck: nil,
            picksUnattended: false,
            offersSharedAlbum: true
        ),
        signals: PreviewData.settledSignals,
        needsYou: .matches(count: 2)
    )

    @Test("several pressings of one album offer Not sure; others do not")
    func severalPressingsOfOneAlbumOfferNotSure() async throws {
        let offered = try await renderedText(of: Self.sharedAlbumState)
        #expect(
            offered.contains {
                $0.localizedCaseInsensitiveContains(
                    String(localized: "Not sure")
                )
            },
            "the pane reads: \(offered)"
        )
        #expect(
            offered.contains {
                $0.localizedCaseInsensitiveContains("these releases share")
            },
            "the pane reads: \(offered)"
        )

        let notOffered = try await renderedText(
            of: PreviewData.searchState(
                identifyState: IdentifyState(
                    bridge: PreviewData.bridgeDisagreementState
                ),
                needsYou: .matches(count: 2)
            )
        )
        #expect(
            !notOffered.contains {
                $0.localizedCaseInsensitiveContains(
                    String(localized: "Not sure")
                )
            },
            "the pane reads: \(notOffered)"
        )
    }

    /// Every line of text the pane draws for `state`.
    private func renderedText(
        of state: ImportSearchState
    ) async throws -> [String] {
        try await FindOnlineRendering.text(
            ImportSearchPane.preview(state: state).importPreviewEnvironment(),
            size: NSSize(width: 900, height: 600)
        )
    }
}
