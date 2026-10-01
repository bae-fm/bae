import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The list's rows selected through the native list: each change reaches core
/// as the selection, a click is never undone by a list read from before it,
/// and Select All asks core for every row the list shows.
@Suite("Import list selection")
struct ImportListSelectionTests {
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
    /// it: read from the store, each read noted in `reads`, and each change
    /// handed to the slot.
    @MainActor
    private static func selectableList(
        store: ImportStore,
        slot: ImportListSlot,
        uiStore: UiStore,
        reads: CallLog<Set<String>>? = nil
    ) -> some View {
        ImportCandidateListContent(
            importStore: store,
            listSlot: slot,
            selectedKeys: Binding(
                get: {
                    let keys = store.shownSelectedKeys
                    reads?.record(keys)
                    return keys
                },
                set: {
                    slot.changeSelection(
                        to: $0,
                        from: store.shownSelectedKeys,
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
                    return 1
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
            let tableView = try Self.table(in: host)
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

    /// A click shows the clicked row selected at once. Core's list catches up
    /// only after the click's write lands, and a list read taken before that —
    /// still selecting the row the click replaced — must not put that row back.
    @MainActor
    @Test(
        "a click stays selected, and a list read from before it can't undo it"
    )
    func aClickIsNotUndoneByAnOlderListRead() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let first = Self.row(PreviewData.triageRowIdentified, selected: true)
        let second = Self.row(
            PreviewData.triageRowUnidentified,
            selected: false
        )
        let written = AsyncStream<Void>.makeStream()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [first, second],
            selection: ImportSelection(
                operations: .stub(change: { _, _ in
                    for await _ in written.stream { break }
                    return 2
                })
            )
        )
        store.applySelectionRevision(1)
        let reads = CallLog<Set<String>>()
        let size = NSSize(width: 400, height: 320)
        try await SnapshotTestSupport.withHostedWindow(
            Self.selectableList(
                store: store,
                slot: slot,
                uiStore: uiStore,
                reads: reads
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let tableView = try Self.table(in: host)
            #expect(tableView.selectedRowIndexes == [0])
            let firstKey = PreviewData.triageRowIdentified.candidateKey
            let secondKey = PreviewData.triageRowUnidentified.candidateKey

            let readsBeforeClick = reads.all.count
            tableView.selectRowIndexes([1], byExtendingSelection: false)
            try await SnapshotTestSupport.settle(host)
            #expect(tableView.selectedRowIndexes == [1])

            // A list read from before the click's write: the first row still
            // selected, at the revision the click started from.
            store.ingest([first, second])
            store.applySelectionRevision(1)
            try await SnapshotTestSupport.settle(host)
            #expect(tableView.selectedRowIndexes == [1])

            // The write lands, and the list read that reflects it.
            written.continuation.yield(())
            store.ingest([
                Self.row(PreviewData.triageRowIdentified, selected: false),
                Self.row(PreviewData.triageRowUnidentified, selected: true),
            ])
            store.applySelectionRevision(2)
            try await SnapshotTestSupport.settle(host)
            #expect(tableView.selectedRowIndexes == [1])
            #expect(store.shownSelectedKeys == [secondKey])

            let afterClick = reads.all.dropFirst(readsBeforeClick)
            #expect(!afterClick.contains([firstKey]))
        }
    }

    /// The table SwiftUI builds for the list.
    @MainActor
    private static func table(in host: NSView) throws -> NSTableView {
        try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTableView }
                .first
        )
    }

    private static func row(
        _ row: BridgeTriageRow,
        selected: Bool
    ) -> BridgeImportListItem {
        var row = row
        row.selected = selected
        return PreviewData.candidateItem(row)
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
            let tableView = try Self.table(in: host)
            #expect(window.makeFirstResponder(tableView))
            try await Wait.until { focused.selectAll != nil }

            let selectAll = try #require(focused.selectAll)
            selectAll.send()

            try await Wait.until { !selectAlls.all.isEmpty }
            #expect(selectAlls.all.count == 1)
            #expect(tableView.selectedRowIndexes.isEmpty)
        }
    }
}

/// The Select All the focused view publishes, as the menu command reads it.
@MainActor
private final class FocusedSelectAll {
    var selectAll: FocusedCommand?
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
