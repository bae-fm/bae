import AppKit
import BaeKit
import Testing

@testable import bae

/// A new value of the view the list already shows: typing into the selected
/// row's pane writes its draft, and the list's next value carries the row as
/// the draft now reads, which the kept key still selects.
@Suite("Import list value for the same view")
struct ImportListSameViewValueTests {
    /// The table keeps the row highlighted, keeps the selection as it was
    /// and leaves the window's focus where it was.
    @MainActor
    @Test("a new value of the same view keeps the selected row and the focus")
    func aNewValueOfTheSameViewKeepsTheSelection() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let selected = PreviewData.triageRowIdentifiedOnline
        let other = PreviewData.candidateItem(PreviewData.triageRowUnidentified)
        let source = MutableImportList(items: [
            PreviewData.candidateItem(selected), other,
        ])
        let slot = ImportListSlot(
            importStore: store,
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { source.handle }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.row(at: 1) != nil }
        slot.selection.select([selected.candidateKey])
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            #expect(table.selectedRowIndexes == [0])
            let window = try #require(table.window)
            let focused = window.firstResponder

            var edited = selected
            let artistLine = try #require(selected.metadataSummary?.artistLine)
            edited.metadataSummary = .titleAndArtists(
                title: "Typed Title",
                artistLine: artistLine
            )
            await source.replaceItems([
                PreviewData.candidateItem(edited), other,
            ])
            try await Wait.until {
                slot.list?.row(at: 0) == PreviewData.candidateItem(edited)
            }
            try await Wait.untilSteady { table.selectedRowIndexes }

            #expect(table.selectedRowIndexes == [0])
            #expect(slot.selection.keys == [edited.candidateKey])
            #expect(window.firstResponder === focused)
        }
    }
}
