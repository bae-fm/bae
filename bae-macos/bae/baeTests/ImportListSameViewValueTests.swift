import AppKit
import BaeKit
import Testing

@testable import bae

/// A new value of the view the list already shows: typing into the selected
/// row's pane writes its draft, and the list's next value carries the row as
/// the draft now reads, still selected at the same revision.
@Suite("Import list value for the same view")
struct ImportListSameViewValueTests {
    /// The table keeps the row highlighted, asks core for nothing and leaves
    /// the window's focus where it was.
    @MainActor
    @Test("a new value of the same view keeps the selected row and the focus")
    func aNewValueOfTheSameViewKeepsTheSelection() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let changes = CallLog<BridgeSelectionChange>()
        var selected = PreviewData.triageRowIdentified
        selected.selected = true
        let other = PreviewData.candidateItem(PreviewData.triageRowUnidentified)
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [PreviewData.candidateItem(selected), other],
            selection: ImportSelection(
                operations: .stub(change: { change in
                    changes.record(change)
                    return 2
                })
            )
        )
        store.applySelectionRevision(1)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            #expect(table.selectedRowIndexes == [0])
            let window = try #require(table.window)
            let focused = window.firstResponder

            var edited = selected
            edited.metadataSummary?.albumTitle = "Typed Title"
            store.ingest([PreviewData.candidateItem(edited), other])
            store.applySelectionRevision(1)
            try await Wait.untilSteady { table.selectedRowIndexes }

            #expect(table.selectedRowIndexes == [0])
            #expect(changes.all.isEmpty)
            #expect(slot.answersView)
            #expect(store.shownSelectedKeys == [edited.candidateKey])
            #expect(window.firstResponder === focused)
        }
    }
}
