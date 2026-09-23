import AppKit
import BaeKit
import SwiftUI
import Testing
import XCTest

@testable import bae

@Suite("Import candidate selection")
struct ImportCandidateSelectionTests {
    @MainActor
    @Test("folder scan activity renders an indeterminate progress control")
    func folderScanActivityRendersIndeterminateProgress() throws {
        let size = NSSize(width: 180, height: 40)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            FolderScanProgressIndicator(
                activity: BridgeFolderScanActivity(
                    foundCount: 179,
                    folders: [
                        BridgeActiveFolderScan(
                            watchedFolderPath: "/imports/incoming",
                            watchedFolderName: "Incoming",
                            foundCount: 179
                        )
                    ]
                )
            )
            .frame(width: size.width, height: size.height),
            size: size
        )

        host.layoutSubtreeIfNeeded()
        let progress = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSProgressIndicator }
                .first
        )
        #expect(progress.isIndeterminate)
    }

    @MainActor
    @Test("a row renders without resolving the outbox environment")
    func rowRendersFromSuppliedUploadPresentation() {
        let size = NSSize(width: 400, height: 80)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: nil,
                uploadObservation: nil,
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )

        host.layoutSubtreeIfNeeded()
        #expect(host.fittingSize.height > 0)
    }

    /// The foot bar offers importing only the selected rows read from a
    /// catalog's release beside importing every selected Ready row, each with
    /// its own count.
    @MainActor
    @Test("the foot bar offers Import identified beside Import ready")
    func footBarOffersImportIdentified() async throws {
        let size = NSSize(width: 460, height: 48)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            TriageFootBar(
                selectedCount: 3,
                selectedIdentifiedCount: 2,
                readyCount: 5,
                onSelectAll: {},
                onSelectNone: {},
                onImport: {},
                onImportIdentified: {}
            )
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        let lines =
            try await SnapshotTestSupport.recognizedText(
                in: SnapshotTestSupport.capturePNG(host, size: size),
                languages: ["en-US"]
            )
            .map(\.text)
        #expect(lines.carrying("Import identified (2)"))
        #expect(lines.carrying("Import ready (3)"))
    }

    @MainActor
    @Test("native row selection is the bulk-action selection")
    func candidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(isGroupMember: false)
    }

    @MainActor
    @Test("native group-member selection is the bulk-action selection")
    func groupedCandidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(isGroupMember: true)
    }

    @MainActor
    private func assertCandidateCanBeSelected(
        isGroupMember: Bool
    ) async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        let store = PreviewData.importTabScene().store
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                .candidate(
                    stableKey:
                        "candidate:\(PreviewData.triageRowReady.candidateKey)",
                    row: PreviewData.triageRowReady,
                    isGroupMember: isGroupMember
                )
            ]
        )
        let size = NSSize(width: 400, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCandidateListContent(
                importStore: store,
                listSlot: slot,
                selectedKeys: Binding(
                    get: { uiStore.selectedFolderCandidates },
                    set: { uiStore.setFolderCandidateSelection($0) }
                ),
                onAddFolder: {},
                onRemoveFolder: { _ in },
                onRefreshFolder: { _ in },
                onReleaseDecision: { _, _ in },
                onSkip: { _, _ in },
                onReveal: { _ in },
                onImportSelected: {},
                onImportIdentified: {}
            )
            .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
            .environment(uiStore)
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        // Exercise the native list selection, not a second checkbox state.
        let tableView = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTableView }
                .first
        )
        tableView.selectRowIndexes(
            IndexSet(integer: 0),
            byExtendingSelection: false
        )
        await SnapshotTestSupport.settle(host)

        #expect(
            uiStore.selectedFolderCandidates
                == [PreviewData.triageRowReady.candidateKey]
        )
    }

}

final class PopoverAnimationTests: XCTestCase {
    @MainActor
    func testPopoverBehaviorDisablesEnclosingPopoverAnimation() async {
        let size = NSSize(width: 80, height: 40)
        let (window, anchor) = SnapshotTestSupport.hostInWindow(
            Color.clear.frame(width: size.width, height: size.height),
            size: size
        )
        let popover = NSPopover()
        popover.animates = true
        let contentViewController = NSHostingController(
            rootView: PopoverBehavior()
                .frame(width: 120, height: 80)
        )
        popover.contentViewController = contentViewController
        popover.show(
            relativeTo: anchor.bounds,
            of: anchor,
            preferredEdge: .maxY
        )

        await SnapshotTestSupport.settle(contentViewController.view)

        XCTAssertFalse(popover.animates)
        popover.performClose(nil)
        withExtendedLifetime(window) {}
    }
}
