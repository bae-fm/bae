import AppKit
import Foundation
import Testing

@testable import BaeKit
@testable import bae

@Suite("Import list entry cell")
@MainActor
struct ImportListEntryCellTests {
    @Test("a redraw of the same cover keeps its load running")
    func redrawKeepsTheCoverLoad() async throws {
        let fetches = Counter()
        let images = ImageStore(fetchLibraryImageBytes: { _ in
            fetches.increment()
            Wait.happened()
            try await Task.sleep(for: .seconds(600))
            return nil
        })
        let cell = ImportListEntryCell(
            identifier: NSUserInterfaceItemIdentifier("candidate")
        )
        let first = Self.entry(cover: "cover-1", images: images)

        cell.show(first, displayScale: 2)
        let load = try #require(cell.coverLoad)
        try await Wait.until { fetches.count == 1 }
        cell.show(first, displayScale: 2)
        cell.show(Self.entry(cover: "cover-1", images: images), displayScale: 2)
        #expect(cell.coverLoad == load, "the redraws left the load alone")

        cell.show(Self.entry(cover: "cover-2", images: images), displayScale: 2)
        #expect(cell.coverLoad != load, "another cover starts its own load")
        try await Wait.until { fetches.count == 2 }

        // No scale ends the cell's load.
        cell.show(first, displayScale: nil)
        #expect(cell.coverLoad == nil)
    }

    private static func entry(
        cover: String,
        images: ImageStore
    ) -> ImportListEntry {
        let row = BridgeTriageRow(
            candidateKey: Data("/library/release".utf8),
            folderName: "Release",
            watchedFolderPath: Data("/library".utf8),
            actionable: true,
            placement: .skipped,
            live: PreviewData.triageLive([]),
            metadataSummary: nil,
            cover: nil,
            importStatus: nil,
            metadataProvenance: nil,
            reading: .unidentified
        )
        return ImportListEntry(
            item: PreviewData.candidateItem(row),
            coverContent: .libraryImage(
                BridgeImageRef(id: cover, version: "1", imageType: .cover)
            ),
            actions: ImportListEntryActions(
                onSetGroupExpanded: { _, _ in },
                onCombineFolder: { _ in },
                onSeparate: { _ in },
                onShowInLibrary: { _ in },
                onPerform: { _ in },
                menuOffers: { _ in .empty }
            ),
            environment: ImportListRowEnvironment(
                imageStore: images,
                importer: nil,
                importPercents: nil,
                outboxStore: OutboxStore(uploadsPaused: false),
                appearance: HostedAppearance(tone: .standard, accent: .standard)
            )
        )
    }
}
