import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A pick can wait on a catalog over the network, so the row it was made on
/// shows the step that the catalog apply's row holds while it waits. The run's own pick
/// says its release is being read in a line of SwiftUI text, which the
/// `import-search-reading-pick` shot shows.
@MainActor
@Suite("Pick step line")
struct PickStepLineTests {
    @Test("a pick reading from a catalog shows its step on its row")
    func readingNamesTheCatalog() async throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )
        let size = NSSize(width: 620, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSearchResultRow(
                pressing: pressing,
                isImporting: false,
                isSelected: false,
                loading: .followed(
                    FixedCatalogApply(
                        step: .readingRelease(catalog: .discogs)
                    )
                ),
                onSelect: { _ in }
            )
            .frame(width: size.width)
            .frame(width: size.width, height: size.height, alignment: .top)
            .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let shown = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is ProgressTrackNSView && !$0.visibleRect.isEmpty }
            #expect(shown.count == 1, "showed \(shown)")
        }
    }
}
