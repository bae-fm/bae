import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What the remote section says of its lookup. The words for a lookup
/// nothing is linked to, one that found nothing, and one that failed are the
/// `cover-picker-unlinked`, `cover-picker-none-found` and
/// `cover-picker-lookup-failed` shots'; a lookup still out spins.
@MainActor
struct CoverPickerEmptyStateTests {
    /// The saving line's spinner is laid out in every state, hidden while
    /// nothing saves, so a lookup still out is the one more.
    @Test("Only a lookup still out spins")
    func lookupSpinsWhileOut() async throws {
        let settled = [
            RemoteCoverItems.unlinked, .linked([]),
            .failed([], failure: DisplayError(line: "Artwork lookup failed")),
        ]
        let resting = try await spinners(.linked([]))
        for remoteItems in settled {
            #expect(
                try await spinners(remoteItems) == resting,
                "\(remoteItems)"
            )
        }
        #expect(try await spinners(.loading([])) == resting + 1)
    }

    private func spinners(_ remoteItems: RemoteCoverItems) async throws -> Int {
        let size = NSSize(width: 960, height: 700)
        return try await SnapshotTestSupport.withHostedWindow(
            CoverGalleryView(
                remoteItems: remoteItems,
                releaseItems: [],
                selectedCover: nil,
                lightboxLoad: LightboxImageLoad(),
                onRefresh: {},
                onFindRelease: {},
                onSelect: { _ in },
                onDone: {}
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
                .count
        }
    }

    /// An unlinked candidate has no lookup to refresh, and its release files
    /// stay in the browser beside the offer to find its release. Pressing
    /// Find release… runs `onFindRelease` itself; where the button sits is
    /// no longer tested.
    @Test(
        "An unlinked candidate offers identification while retaining release files"
    )
    func identifyFromPicker() throws {
        #expect(!RemoteCoverItems.unlinked.canRefresh)
        let images = PreviewData.bridgeCandidateFiles.images
        let files = images.compactMap { file in
            file.coverChoice.map {
                CoverItem(coverChoice: $0, label: file.file.name)
            }
        }
        try #require(!files.isEmpty && files.count == images.count)
        var browser = ArtworkBrowserState(layout: .grid)
        browser.update(
            currentCover: nil,
            remoteItems: RemoteCoverItems.unlinked.items,
            releaseItems: files,
            selectedCover: nil
        )
        #expect(browser.showsReleaseFiles)
        #expect(browser.releaseItems == files)
    }
}
