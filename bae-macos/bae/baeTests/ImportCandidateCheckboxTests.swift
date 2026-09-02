import AppKit
import BaeKit
import SwiftUI
import Testing
import XCTest

@testable import bae

@Suite("Import candidate checkbox")
struct ImportCandidateCheckboxTests {
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
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )

        host.layoutSubtreeIfNeeded()
        #expect(host.fittingSize.height > 0)
    }

    @MainActor
    @Test("an unchecked candidate can be checked inside the selectable list")
    func uncheckedCandidateCanBeChecked() async throws {
        try await assertCheckboxCanBeChecked(isGroupMember: false)
    }

    @MainActor
    @Test("a group member checkbox stays inside the selectable list")
    func groupedCandidateCanBeChecked() async throws {
        try await assertCheckboxCanBeChecked(isGroupMember: true)
    }

    @MainActor
    private func assertCheckboxCanBeChecked(
        isGroupMember: Bool
    ) async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        // One Ready row, so the first checkbox in the rendered list is the one
        // this test clicks. Which rows a tab holds is core's answer now, so a
        // narrower list is stated as a narrower fixture rather than as a
        // filter the view applies.
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
        let listSelection = CandidateListSelection()
        let size = NSSize(width: 400, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCandidateListContent(
                importStore: store,
                listSlot: slot,
                selectedKeys: listSelection.binding,
                onAddFolder: {},
                onRemoveFolder: { _ in },
                onRefreshFolder: { _ in },
                onReleaseDecision: { _, _ in },
                onSkip: { _, _ in },
                onImportSelected: { _ in }
            )
            .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
            .environment(uiStore)
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        // SwiftUI's List is table-backed, so the table's own geometry says
        // where the row is; the checkbox sits centered in the row's trailing
        // edge padding.
        let tableView = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTableView }
                .first
        )
        let cell = try #require(
            tableView.view(atColumn: 0, row: 0, makeIfNecessary: false)
        )
        let cellRect = cell.convert(cell.bounds, to: nil)
        let point = NSPoint(
            x: cellRect.maxX - ImportListHierarchyLayout.rowEdgePadding - 7,
            y: cellRect.midY
        )
        try click(at: point, in: window)
        await Task.yield()

        #expect(
            uiStore.selectedReadyCandidates
                == [PreviewData.triageRowReady.candidateKey]
        )
    }

    @MainActor
    private func click(at point: NSPoint, in window: NSWindow) throws {
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
                NSEvent.mouseEvent(
                    with: type,
                    location: point,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            window.sendEvent(event)
        }
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

@MainActor
private final class CandidateListSelection {
    private var keys: Set<String> = []

    var binding: Binding<Set<String>> {
        Binding(
            get: { self.keys },
            set: { self.keys = $0 }
        )
    }
}
