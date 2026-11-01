import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A pick can wait on a catalog over the network, so the row it was made on
/// names the catalog it is reading from while it does.
@MainActor
@Suite("Pick step line")
struct PickStepLineTests {
    @Test("a pick reading from a catalog names it on its row")
    func readingNamesTheCatalog() async throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )
        let size = NSSize(width: 620, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSearchResultRow(
                pressing: pressing,
                isImporting: false,
                libraryStatus: nil,
                isSelected: false,
                loading: LoadingPick(
                    releaseId: pressing.lead.releaseId,
                    reading: .followed(
                        LatestStream.catalogApplySteps(
                            holding: .readingRelease(catalog: .discogs)
                        )
                        .only
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
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(
                lines.carrying("Reading the release from Discogs"),
                "drew \(lines)"
            )
        }
    }

    @Test("the run's own pick says its release is being read")
    func unattendedReadIsNamed() async throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )
        let size = NSSize(width: 620, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSearchResultRow(
                pressing: pressing,
                isImporting: false,
                libraryStatus: nil,
                isSelected: true,
                loading: LoadingPick(
                    releaseId: pressing.lead.releaseId,
                    reading: .unattended
                ),
                onSelect: { _ in }
            )
            .frame(width: size.width)
            .frame(width: size.width, height: size.height, alignment: .top)
            .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(
                lines.carrying("Reading the release in full"),
                "drew \(lines)"
            )
        }
    }
}
