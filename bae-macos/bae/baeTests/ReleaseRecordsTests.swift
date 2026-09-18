import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Which catalogs describe a release is one answer, drawn once: as a row of
/// links last in the import pane, and behind the library expansion's facts
/// line.
@MainActor
@Suite("Release records")
struct ReleaseRecordsTests {
    // Tall enough that the pane's whole scrolling content renders: the
    // records row is the last thing in it.
    private static let paneSize = NSSize(width: 900, height: 1800)
    private static let rowSize = NSSize(width: 420, height: 120)
    private static let cardSize = NSSize(width: 1100, height: 760)

    /// The row names every catalog that describes the release, whichever of
    /// them the draft was read from.
    @Test("the records row names every catalog")
    func theRecordsRowNamesEveryCatalog() async throws {
        let lines = try await FindOnlineRendering.text(
            ReleaseRecordsRow(records: PreviewData.releaseRecordsEveryCatalog)
                .padding()
                .frame(width: Self.rowSize.width)
                .preferredColorScheme(.light)
                .background(.white),
            size: Self.rowSize
        )
        for catalog in bridgeCatalogs() {
            let name = bridgeCatalogName(catalog: catalog)
            #expect(
                lines.carrying(name),
                "\(name) is missing from \(lines)"
            )
        }
    }

    /// The pane draws the records once, at its end. The header above them
    /// names no catalog: the artist line used to carry a chip per source, and
    /// two places saying the same thing is what the row replaced.
    @Test("the pane names its catalogs in the records row, not on the header")
    func thePaneNamesItsCatalogsOnce() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable,
            reading: .identified(records: PreviewData.releaseRecordsPair)
        )
        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        let lines = try await FindOnlineRendering.text(
            ImportMappingPreview.make(
                candidate: candidate,
                storageCloud: .constant(false),
                storagePinned: .constant(false)
            )
            .environment(store)
            .importPreviewEnvironment()
            .preferredColorScheme(.light),
            size: Self.paneSize
        )
        for catalog in ["MusicBrainz", "Discogs"] {
            #expect(
                lines.filter { $0.contains(catalog) }.count == 1,
                "\(catalog) is named once, in the records row: \(lines)"
            )
        }
    }

    /// The expansion is untouched at rest: the facts line becomes a trigger
    /// when a catalog describes the release, and a trigger that is not being
    /// pointed at draws nothing of its own.
    @Test("the expansion card reads the same at rest with and without records")
    func theExpansionIsUntouchedAtRest() async throws {
        let store = PreviewData.seededLibraryStore()
        let summary = try #require(store.albumSummaries["a-01"])
        let described = try #require(
            store.releaseDetails[summary.releaseIds[0]]
        )
        #expect(!described.records.isEmpty, "the fixture carries records")
        var undescribed = described
        undescribed.records = []

        let withRecords = try await pixels(of: described, in: summary, store)
        let without = try await pixels(of: undescribed, in: summary, store)
        // Compared as one answer rather than as two collections: an
        // inequality between megabyte-sized captures is not a diff worth
        // computing.
        let identical = withRecords == without
        #expect(identical, "the records leave the card as it was")
    }

    private func pixels(
        of release: ReleaseDetail,
        in summary: AlbumSummary,
        _ store: LibraryStore
    ) async throws -> Data {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            PreviewData.albumExpansionContent(
                summary: summary,
                selectedRelease: release,
                releaseCursor: .constant(
                    PreviewData.releaseCursor(
                        releaseIds: summary.releaseIds,
                        preferring: release.id
                    )
                )
            )
            .frame(width: Self.cardSize.width)
            .preferredColorScheme(.light)
            .background(.white)
            .environment(UiStore())
            .environment(store)
            .environment(ImageStore.stub()),
            size: Self.cardSize
        )
        defer { withExtendedLifetime(window) {} }
        await SnapshotTestSupport.settle(host)
        return try await SnapshotTestSupport.capturePNG(
            host,
            size: Self.cardSize,
            // The card's art loads off the main actor, and a capture taken
            // before it lands reads differently from one taken after — a
            // difference about the image, not about the records.
            waitNanoseconds: 200_000_000
        )
    }
}
