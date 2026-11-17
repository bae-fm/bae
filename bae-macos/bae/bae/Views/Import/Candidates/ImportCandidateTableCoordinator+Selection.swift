import AppKit
import BaeKit

// MARK: - Selection

/// The screen keeps the selection as the keys of the rows the person
/// selected, and each row the table draws takes its highlight from them: the
/// table's own selection handling only proposes rows, and each proposal is
/// read as the change it makes to the kept keys. A Shift run and Select All
/// take their keys from core, which reads every row between two rows, or
/// every row the list shows, loaded or not.
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
            kept: listSlot.selection.keys,
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
        case .change(let kept, let run, let anchor):
            anchorKey = anchor
            endPendingPlace()
            onPersonSelected()
            listSlot.selection.select(kept)
            if let run {
                listSlot.extendSelection(from: run.from, to: run.to)
            }
            return rows(selecting: kept)
        }
    }

    /// Select every row the list shows, loaded or not: core's keys, not the
    /// table's Select All, which knows only the rows it has loaded.
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
        for (position, _) in list.loadedEntries where position < rowCount {
            guard let key = list.row(at: position)?.selectionKey,
                keys.contains(key)
            else { continue }
            rows.insert(position)
        }
        return rows
    }
}
