import AppKit
import BaeKit

// MARK: - Selection

/// The table's highlight is core's selection, and what a person does to the
/// rows goes to core: the table's own selection handling only proposes rows,
/// and each proposal is read as the change it asks of core's selection.
extension ImportCandidateTableCoordinator {
    func tableView(
        _ tableView: NSTableView,
        selectionIndexesForProposedSelection proposed: IndexSet
    ) -> IndexSet {
        let table = tableView as? ImportCandidateTableView
        // With no press being handled, no keys are held.
        let event = table?.handlingEvent
        let request = ImportListSelectionRequest(
            proposed: proposed,
            current: tableView.selectedRowIndexes,
            shown: importStore.shownSelectedKeys,
            gesture: SelectionGesture(event?.modifierFlags ?? []),
            target: event.flatMap { table?.pressedRow($0) },
            anchor: anchorKey,
            key: { [self] in item(at: $0)?.selectionKey }
        )
        switch request.answer {
        case .rejected:
            // An arrow key moving onto a header goes on to the row past it
            // when nothing is accepted; a click on one changes nothing.
            return event?.type == .keyDown ? [] : tableView.selectedRowIndexes
        case .unchanged:
            return tableView.selectedRowIndexes
        case .change(let change, let shown, let anchor):
            anchorKey = anchor
            endPendingPlace()
            onPersonSelected()
            listSlot.changeSelection(change, showing: shown)
            return rows(selecting: shown)
        }
    }

    /// Select every row the list shows, loaded or not: core's Select All,
    /// not the table's, which knows only the rows it has loaded.
    func selectAllShown() {
        onPersonSelected()
        listSlot.selectAllShown()
    }

    /// Highlight the loaded rows `keys` names, and no other.
    func highlight(_ keys: Set<Data>) {
        guard let tableView else { return }
        let rows = rows(selecting: keys)
        guard rows != tableView.selectedRowIndexes else { return }
        tableView.selectRowIndexes(rows, byExtendingSelection: false)
    }

    /// The positions of the loaded rows whose candidates `keys` names.
    private func rows(selecting keys: Set<Data>) -> IndexSet {
        var rows = IndexSet()
        guard !keys.isEmpty else { return rows }
        for (position, stableKey) in list.loadedEntries
        where position < rowCount {
            guard let key = importStore.items[stableKey]?.selectionKey,
                keys.contains(key)
            else { continue }
            rows.insert(position)
        }
        return rows
    }
}
