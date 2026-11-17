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
                content: CoverGalleryContent(
                    BridgeCoverGallery(
                        current: nil,
                        fetch: .notFetched,
                        sections: [
                            BridgeCoverSection(
                                source: .remote,
                                items: [],
                                count: 0
                            )
                        ],
                        total: 0,
                        focus: nil
                    )
                ),
                remoteItems: remoteItems,
                filter: .constant(.all),
                focusedCover: .constant(nil),
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

    /// A lookup nothing is linked to has nothing to refresh; the person can
    /// refresh a lookup that found nothing or failed. Which files show beside the
    /// offer to find the release is core's read
    /// (`a_candidates_picker_counts_what_each_source_shows`).
    @Test("The person can refresh only a linked lookup")
    func refreshOnlyALinkedLookup() {
        #expect(!RemoteCoverItems.unlinked.canRefresh)
        #expect(RemoteCoverItems.linked([]).canRefresh)
        #expect(
            RemoteCoverItems.failed([], failure: DisplayError(line: "failed"))
                .canRefresh
        )
    }
}
