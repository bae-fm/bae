import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import confirmation banners")
struct ImportConfirmationBannersTests {
    private func conflictFailure() -> BridgeImportFailure {
        let discogsArtist = BridgeExistingArtist(
            artistId: "library-artist-1",
            name: "Artist One",
            sortName: nil,
            musicbrainzArtistId: nil,
            discogsArtistId: "discogs-1"
        )
        let musicbrainzArtist = BridgeExistingArtist(
            artistId: "library-artist-2",
            name: "Artist One",
            sortName: nil,
            musicbrainzArtistId: "musicbrainz-1",
            discogsArtistId: nil
        )
        return BridgeImportFailure(
            error: .Diagnostic(
                category: .import,
                detail: "the artist identities disagree"
            ),
            artistIdentityConflict: BridgeArtistIdentityConflict(
                incomingArtistName: "Artist One",
                discogsArtist: discogsArtist,
                musicbrainzArtist: musicbrainzArtist
            )
        )
    }

    @MainActor
    @Test("artist identity repair remains visible beside the failed row status")
    func artistIdentityRepairRemainsVisibleBesideFailedStatus() async {
        let conflictControlCount = await focusControlCount(
            failure: conflictFailure()
        )
        let retryControlCount = await focusControlCount(
            failure: BridgeImportFailure(
                error: .Diagnostic(
                    category: .import,
                    detail: "the import failed"
                ),
                artistIdentityConflict: nil
            )
        )

        #expect(conflictControlCount == retryControlCount + 1)
    }

    @MainActor
    private func focusControlCount(
        failure: BridgeImportFailure
    ) async -> Int {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportConfirmationBanners(
                libraryStatus: nil,
                importStatus: .error(
                    error: .Diagnostic(
                        category: .import,
                        detail: "the artist identities disagree"
                    )
                ),
                error: nil,
                failure: failure,
                canEdit: true,
                onRetry: {},
                onMergeArtists: { _ in },
                onViewInLibrary: { _ in }
            ),
            size: NSSize(width: 640, height: 240)
        )
        await SnapshotTestSupport.settle(host)
        let count = host.subviews
            .filter {
                $0.nextKeyView != nil || $0.previousKeyView != nil
            }
            .count
        withExtendedLifetime(window) {}
        return count
    }
}
