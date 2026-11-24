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
        let images = ImageStore(fetchLibraryImageBytes: { _, _ in
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
        let load = try #require(cell.coverDecode?.task)
        try await Wait.until { fetches.count == 1 }
        cell.show(first, displayScale: 2)
        cell.show(Self.entry(cover: "cover-1", images: images), displayScale: 2)
        #expect(
            cell.coverDecode?.task == load,
            "the redraws left the load alone"
        )

        cell.show(Self.entry(cover: "cover-2", images: images), displayScale: 2)
        #expect(
            cell.coverDecode?.task != load,
            "another cover starts its own load"
        )
        try await Wait.until { fetches.count == 2 }

        // No scale ends the cell's load.
        cell.show(first, displayScale: nil)
        #expect(cell.coverDecode?.task == nil)
    }

    @Test("a row's spinner holds still in a window that holds spinners still")
    func spinnerHoldsStillWithItsWindow() async throws {
        let cell = ImportListEntryCell(
            identifier: NSUserInterfaceItemIdentifier("candidate")
        )
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 420, height: 70),
            styleMask: [.titled],
            backing: .buffered,
            defer: true
        )
        window.isReleasedWhenClosed = false
        window.holdsSpinnersStill = true
        cell.frame = NSRect(x: 0, y: 0, width: 420, height: 70)
        window.contentView?.addSubview(cell)
        cell.show(
            Self.entry(
                cover: "cover-1",
                images: ImageStore(fetchLibraryImageBytes: { _, _ in nil }),
                identifying: .running
            ),
            displayScale: 2
        )
        // SwiftUI builds the spinner when the cell lays out.
        try await Wait.until {
            cell.layoutSubtreeIfNeeded()
            return cell.firstDescendant(ofType: MacSpinner.NativeIndicator.self)
                != nil
        }
        let indicator = try #require(
            cell.firstDescendant(ofType: MacSpinner.NativeIndicator.self)
        )
        #expect(!indicator.animating)
        window.holdsSpinnersStill = false
        #expect(indicator.animating)
    }

    private static func entry(
        cover: String,
        images: ImageStore,
        identifying: BridgeIdentifyingStep? = nil
    ) -> ImportListEntry {
        let row = BridgeTriageRow(
            candidateKey: Data("/library/release".utf8),
            folderName: "Release",
            watchedFolderPath: Data("/library".utf8),
            actionable: true,
            placement: .skipped,
            live: PreviewData.triageLive([], identifying: identifying),
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
