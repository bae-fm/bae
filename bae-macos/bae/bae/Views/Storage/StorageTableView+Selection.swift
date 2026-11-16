import AppKit
import BaeKit

// MARK: - Selection

/// The Storage Manager keeps its selection as release ids in the view, and
/// each row in view draws its highlight from them: the table's own index
/// selection only proposes rows, and each proposal is read as a change to
/// the kept ids. A Shift run and Select All take their ids from core, which
/// reads every release between two releases, or every release the list
/// shows, loaded or not.
extension StorageTableView.Coordinator {
    func tableView(
        _: NSTableView,
        selectionIndexesForProposedSelection proposed: IndexSet
    ) -> IndexSet {
        // A highlight the coordinator sets is no person's change.
        guard !applyingSelection else { return proposed }
        // With no press being handled, no keys are held.
        let event = table.handlingEvent
        let target = event.flatMap { table.pressedRow($0) }
        let proposedIds = ids(at: proposed)
        // A proposal of rows still loading selects nothing.
        guard proposed.isEmpty || !proposedIds.isEmpty else {
            return table.selectedRowIndexes
        }
        let current = table.selectedRowIndexes
        let kept = selection.wrappedValue
        switch SelectionGesture(event?.modifierFlags ?? []) {
        case .replace:
            anchorId =
                target.flatMap { list.row(at: $0)?.id }
                ?? (proposedIds.count == 1 ? proposedIds.first : anchorId)
            selection.wrappedValue = proposedIds
        case .toggle:
            let add = ids(at: proposed.subtracting(current))
            let remove = ids(at: current.subtracting(proposed))
            anchorId = target.flatMap { list.row(at: $0)?.id } ?? anchorId
            selection.wrappedValue = kept.union(add).subtracting(remove)
        case .extend:
            extend(proposed: proposed, current: current, target: target)
        }
        return highlightedRows(selection.wrappedValue)
    }

    /// A Shift gesture: the rows it proposes show selected at once, and
    /// core reads every release from the anchor to the row it reaches,
    /// loaded or not, which joins the kept ids. One that only shrinks the
    /// run takes out the rows it drops.
    private func extend(proposed: IndexSet, current: IndexSet, target: Int?) {
        let kept = selection.wrappedValue
        let added = proposed.subtracting(current)
        guard let first = added.first, let last = added.last else {
            let remove = ids(at: current.subtracting(proposed))
            selection.wrappedValue = kept.subtracting(remove)
            return
        }
        let end = target ?? (last > (current.last ?? -1) ? last : first)
        guard let to = list.row(at: end)?.id else { return }
        let from = anchorId ?? to
        anchorId = from
        selection.wrappedValue = kept.union(ids(at: proposed))
        let list = list
        Task { [weak self, storageManagerStore] in
            guard
                let run = await storageManagerStore.idsBetween(
                    from: from,
                    to: to
                ),
                // A tab, sort or search change since swapped the list, and
                // with it what the run means.
                let self, list === self.list
            else { return }
            self.selection.wrappedValue.formUnion(run)
        }
    }

    /// Select every release the list shows, loaded or not: core's ids, not
    /// the table's Select All, which knows only the rows it has loaded.
    func selectAllShown() {
        let list = list
        Task { [weak self, storageManagerStore] in
            guard let ids = await storageManagerStore.allIds(),
                let self, list === self.list
            else { return }
            self.selection.wrappedValue = Set(ids)
        }
    }

    /// Highlight the rows in view whose releases `kept` names, each from the
    /// row it draws. Guarded so the table's delegate call that follows
    /// doesn't read it as a person's change.
    func applySelection(_ kept: Set<String>) {
        let rows = highlightedRows(kept)
        guard rows != table.selectedRowIndexes else { return }
        applyingSelection = true
        table.selectRowIndexes(rows, byExtendingSelection: false)
        applyingSelection = false
    }

    /// The releases of the rows the table draws at `rows`; a row still
    /// loading names none.
    private func ids(at rows: IndexSet) -> Set<String> {
        Set(rows.compactMap { list.row(at: $0)?.id })
    }

    /// The rows in view whose releases `kept` names.
    private func highlightedRows(_ kept: Set<String>) -> IndexSet {
        var rows = IndexSet()
        guard !kept.isEmpty else { return rows }
        let range = table.rows(in: table.visibleRect)
        for row in range.location..<(range.location + range.length)
        where row < rootCount {
            if let id = list.row(at: row)?.id, kept.contains(id) {
                rows.insert(row)
            }
        }
        return rows
    }
}
