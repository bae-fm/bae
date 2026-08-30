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
        let (_, host) = SnapshotTestSupport.hostInWindow(
            ImportConfirmationBanners(
                libraryStatus: nil,
                importStatus: .error(
                    error: .Diagnostic(
                        category: .import,
                        detail: "the artist identities disagree"
                    )
                ),
                error: nil,
                failure: conflictFailure(),
                onRetry: {},
                onMergeArtists: { _ in },
                onViewInLibrary: { _ in }
            ),
            size: NSSize(width: 640, height: 240)
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let labels = SnapshotTestSupport.descendants(of: host)
            .compactMap { ($0 as? NSTextField)?.stringValue }
        #expect(
            labels.contains(
                coreString(
                    "ui.import.artist_identity_conflict.keep_discogs"
                )
            )
        )
        #expect(
            labels.contains(
                coreString(
                    "ui.import.artist_identity_conflict.keep_musicbrainz"
                )
            )
        )
    }

}
