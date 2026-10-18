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
        return .artistIdentityConflict(
            conflict: BridgeArtistIdentityConflict(
                incomingArtistName: "Artist One",
                discogsArtist: discogsArtist,
                musicbrainzArtist: musicbrainzArtist
            ),
            error: .Diagnostic(
                category: .import,
                detail: "the artist identities disagree"
            )
        )
    }

    @MainActor
    @Test("artist identity repair remains visible beside the failed row status")
    func artistIdentityRepairRemainsVisibleBesideFailedStatus() async throws {
        let conflictControlCount = try await focusControlCount(
            failure: conflictFailure()
        )
        let retryControlCount = try await focusControlCount(
            failure: .other(
                error: .Diagnostic(
                    category: .import,
                    detail: "the import failed"
                )
            )
        )

        #expect(conflictControlCount == retryControlCount + 4)
    }

    @MainActor
    @Test("keyboard activation sends all five choices; none is automatic")
    func eachButtonSendsItsChoice() async throws {
        var choices: [BridgeArtistIdentityConflictChoice] = []
        try await SnapshotTestSupport.withHostedWindow(
            ImportConfirmationBanners(
                libraryStatus: nil,
                importStatus: nil,
                error: nil,
                failure: conflictFailure(),
                canEdit: true,
                onRetry: { Issue.record("a conflict does not offer retry") },
                onChooseArtistIdentity: { choices.append($0) },
                onViewInLibrary: { _ in }
            ),
            size: NSSize(width: 720, height: 480)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            #expect(choices.isEmpty)
            let controls = host.subviews.filter {
                $0.nextKeyView != nil || $0.previousKeyView != nil
            }
            // The banner also has selectable diagnostic text and its disclosure
            // button. Traverse the whole focus order; only the five choices
            // may send an artist answer.
            for control in controls {
                #expect(HostedInput.focus(control, in: window))
                try HostedInput.keyDown(.space, in: window)
                try await SnapshotTestSupport.settle(host)
            }
            #expect(
                choices == [
                    .sameArtist, .discogsArtist, .musicBrainzArtist, .neither,
                    .changeMatch,
                ]
            )
        }
    }

    @MainActor
    private func focusControlCount(
        failure: BridgeImportFailure
    ) async throws -> Int {
        return try await SnapshotTestSupport.withHostedWindow(
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
                onChooseArtistIdentity: { _ in },
                onViewInLibrary: { _ in }
            ),
            size: NSSize(width: 640, height: 480)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let count = host.subviews
                .filter {
                    $0.nextKeyView != nil || $0.previousKeyView != nil
                }
                .count
            return count
        }
    }
}
