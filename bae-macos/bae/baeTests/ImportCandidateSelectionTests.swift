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
    func folderScanActivityRendersIndeterminateProgress() async throws {
        let size = NSSize(width: 180, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
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
        ) { _, host in

            host.layoutSubtreeIfNeeded()
            let progress = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSProgressIndicator }
                    .first
            )
            #expect(progress.isIndeterminate)
        }
    }

    @MainActor
    @Test("a row renders without resolving the outbox environment")
    func rowRendersFromSuppliedUploadPresentation() async throws {
        let size = NSSize(width: 400, height: 80)
        try await SnapshotTestSupport.withHostedWindow(
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: nil,
                isGroupMember: false
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            host.layoutSubtreeIfNeeded()
            #expect(host.fittingSize.height > 0)
        }
    }

    /// What is running for a candidate reaches its row through the row's own
    /// subscription, asked with the row's basis: a run the subscription says
    /// is going draws the row's spinner, and the list's row alone draws none.
    @MainActor
    @Test("a row draws the run its own subscription reports")
    func rowDrawsItsOwnLiveState() async throws {
        let row = PreviewData.triageRowUnidentified
        let asked = RecordedLiveStateSubscription()
        let running = Importer(
            subscribeCandidateLiveState: { key, basis, callback in
                asked.record(key: key, basis: basis)
                callback.onValue(
                    value: BridgeCandidateLiveState(
                        identification: .running,
                        importing: false,
                        actions: [.skip]
                    )
                )
                return asked
            }
        )
        func spinners(_ importer: Importer) async throws -> Int {
            let size = NSSize(width: 400, height: 80)
            return try await SnapshotTestSupport.withHostedWindow(
                TriageRowView(
                    row: row,
                    coverContent: nil,
                    isGroupMember: false
                )
                .environment(importer)
                .environment(ImageStore.stub())
                .frame(width: size.width, height: size.height),
                size: size
            ) { _, host in
                try await SnapshotTestSupport.settle(host)
                return SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSProgressIndicator }
                    .count
            }
        }

        #expect(try await spinners(Importer()) == 0)
        #expect(try await spinners(running) == 1)
        #expect(asked.keys == [row.candidateKey])
        #expect(asked.bases == [row.actionBasis])
    }

    @MainActor
    @Test("native row selection reaches core as the selection")
    func candidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(isGroupMember: false)
    }

    @MainActor
    @Test("native group-member selection reaches core as the selection")
    func groupedCandidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(isGroupMember: true)
    }

    /// The list with its rows' selection wired the way the import view wires
    /// it: read from core's rows, and each change handed to the slot.
    @MainActor
    private static func selectableList(
        store: ImportStore,
        slot: ImportListSlot,
        uiStore: UiStore
    ) -> some View {
        ImportCandidateListContent(
            importStore: store,
            listSlot: slot,
            selectedKeys: Binding(
                get: { store.selectedLoadedKeys },
                set: {
                    slot.changeSelection(
                        to: $0,
                        from: store.selectedLoadedKeys,
                        by: .replace
                    )
                }
            ),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onReveal: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(ImportSelection())
        .environment(ImageStore.stub())
    }

    @MainActor
    private func assertCandidateCanBeSelected(
        isGroupMember: Bool
    ) async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        let store = PreviewData.importTabScene().store
        let changes = CallLog<BridgeSelectionChange>()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                .candidate(
                    stableKey:
                        "candidate:\(PreviewData.triageRowIdentified.candidateKey)",
                    row: PreviewData.triageRowIdentified,
                    isGroupMember: isGroupMember
                )
            ],
            selection: ImportSelection(
                operations: .stub(change: { _, change in
                    changes.record(change)
                })
            )
        )
        let size = NSSize(width: 400, height: 320)
        try await SnapshotTestSupport.withHostedWindow(
            Self.selectableList(store: store, slot: slot, uiStore: uiStore)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

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
            try await SnapshotTestSupport.settle(host)

            try await Wait.until { !changes.all.isEmpty }
            #expect(
                changes.all == [
                    .replace(keys: [
                        PreviewData.triageRowIdentified.candidateKey
                    ])
                ]
            )
        }
    }

    /// Edit ▸ Select All and Command-A run the command the focused list
    /// publishes, and the list's asks core for every row its view shows
    /// rather than the table's loaded rows.
    @MainActor
    @Test("the focused list's Select All asks core for every shown row")
    func focusedListSelectAllAsksCore() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let selectAlls = CallLog<BridgeImportListView>()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                PreviewData.candidateItem(PreviewData.triageRowIdentified)
            ],
            selection: ImportSelection(
                operations: .stub(selectAll: { selectAlls.record($0) })
            )
        )
        let focused = FocusedSelectAll()
        let size = NSSize(width: 400, height: 320)
        try await SnapshotTestSupport.withHostedWindow(
            Self.selectableList(store: store, slot: slot, uiStore: uiStore)
                .background { FocusedSelectAllReader(focused: focused) }
                .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            #expect(focused.selectAll == nil)
            let tableView = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTableView }
                    .first
            )
            #expect(window.makeFirstResponder(tableView))
            try await Wait.until { focused.selectAll != nil }

            let selectAll = try #require(focused.selectAll)
            selectAll()

            try await Wait.until { !selectAlls.all.isEmpty }
            #expect(selectAlls.all.count == 1)
            #expect(tableView.selectedRowIndexes.isEmpty)
        }
    }

}

/// The Select All the focused view publishes, as the menu command reads it.
@MainActor
private final class FocusedSelectAll {
    var selectAll: (() -> Void)?
}

private struct FocusedSelectAllReader: View {
    let focused: FocusedSelectAll
    @FocusedValue(\.selectAllShownRows)
    private var selectAll

    var body: some View {
        Color.clear.onChange(of: selectAll == nil, initial: true) {
            focused.selectAll = selectAll
        }
    }
}

final class PopoverAnimationTests: XCTestCase {
    @MainActor
    func testPopoverBehaviorDisablesEnclosingPopoverAnimation() async throws {
        let size = NSSize(width: 80, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear.frame(width: size.width, height: size.height),
            size: size
        ) { window, anchor in
            let popover = NSPopover()
            popover.animates = true
            let contentViewController = NSHostingController(
                rootView: PopoverBehavior()
                    .frame(width: 120, height: 80)
            )
            popover.contentViewController = contentViewController
            // A popover is placed on a display whatever its anchor's window
            // is, so the one this test opens is shown transparent to the eye
            // and to the pointer; the test reads the popover, not its pixels.
            let observer = NotificationCenter.default.addObserver(
                forName: NSPopover.willShowNotification,
                object: popover,
                queue: nil
            ) { _ in
                MainActor.assumeIsolated {
                    let window = contentViewController.view.window
                    window?.alphaValue = 0
                    window?.ignoresMouseEvents = true
                }
            }
            defer { NotificationCenter.default.removeObserver(observer) }
            popover.show(
                relativeTo: anchor.bounds,
                of: anchor,
                preferredEdge: .maxY
            )

            try await SnapshotTestSupport.settle(contentViewController.view)

            XCTAssertFalse(popover.animates)
            popover.performClose(nil)
        }
    }
}

/// Records what a row asked its live-state subscription for.
private final class RecordedLiveStateSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    private let lock = NSLock()
    private var asked: [(String, BridgeCandidateActionBasis)] = []

    var keys: [String] { lock.withLock { asked.map(\.0) } }
    var bases: [BridgeCandidateActionBasis] { lock.withLock { asked.map(\.1) } }

    func record(key: String, basis: BridgeCandidateActionBasis) {
        lock.withLock { asked.append((key, basis)) }
    }

    func cancel() {}
}
