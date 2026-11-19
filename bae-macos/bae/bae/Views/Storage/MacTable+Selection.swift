import AppKit
import BaeKit
import OSLog

private let logger = Logger.bae("StorageTableSelection")

// MARK: - Selection

/// The Storage Manager keeps its selection as release ids in the view, and
/// each row in view draws its highlight from them. The table's own index
/// selection only proposes rows, and the coordinator reads each proposal as
/// a change to the kept ids. To select a range with Shift, or to select
/// all, the coordinator asks core, which reads every release between two
/// releases, or every release the list shows, loaded or not.
extension MacTable.Coordinator {
    func tableView(
        _: NSTableView,
        selectionIndexesForProposedSelection proposed: IndexSet
    ) -> IndexSet {
        // A highlight the coordinator sets is no person's change.
        guard !applyingSelection else { return proposed }
        // A proposal that no mouse or key press made carries no modifier
        // keys.
        let event = table.handlingEvent
        let target = event.flatMap { table.pressedRow($0) }
        let proposedIds = ids(at: proposed)
        // A proposal of rows still loading selects nothing, and `ids(at:)`
        // logs the rows.
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
            select(proposedIds)
        case .toggle:
            let add = ids(at: proposed.subtracting(current))
            let remove = ids(at: current.subtracting(proposed))
            anchorId = target.flatMap { list.row(at: $0)?.id } ?? anchorId
            select(kept.union(add).subtracting(remove))
        case .extend:
            extend(proposed: proposed, current: current, target: target)
        }
        return highlightedRows(selection.wrappedValue)
    }

    /// A Shift gesture: the rows it proposes show selected at once, and
    /// core reads every release from the anchor to the row it reaches,
    /// loaded or not, and the coordinator adds them to the kept ids. A
    /// gesture that only shrinks the range takes out the rows it drops.
    private func extend(proposed: IndexSet, current: IndexSet, target: Int?) {
        let kept = selection.wrappedValue
        let added = proposed.subtracting(current)
        guard let first = added.first, let last = added.last else {
            let remove = ids(at: current.subtracting(proposed))
            select(kept.subtracting(remove))
            return
        }
        let end = target ?? (last > (current.last ?? -1) ? last : first)
        guard let to = list.row(at: end)?.id else {
            logger.debug(
                "A Shift gesture to row \(end) selects nothing: the row is still loading"
            )
            return
        }
        let from = anchorId ?? to
        anchorId = from
        select(kept.union(ids(at: proposed)))
        selectionRead = Task { [weak self, storageManagerStore] in
            guard
                let run = await storageManagerStore.idsBetween(
                    from: from,
                    to: to
                ),
                !Task.isCancelled, let self
            else { return }
            self.selection.wrappedValue.formUnion(run)
        }
    }

    /// Select every release the list shows, loaded or not: core's ids, not
    /// the table's Select All, which knows only the rows it has loaded.
    func selectAllShown() {
        cancelSelectionRead()
        selectionRead = Task { [weak self, storageManagerStore] in
            guard let ids = await storageManagerStore.allIds(),
                !Task.isCancelled, let self
            else { return }
            self.selection.wrappedValue = Set(ids)
        }
    }

    /// Keep the ids a gesture selects. A read of core's ids that an earlier
    /// gesture started would overwrite them when it answers, so the
    /// coordinator cancels it.
    func select(_ ids: Set<String>) {
        cancelSelectionRead()
        selection.wrappedValue = ids
    }

    /// Cancel the read of core's ids that a Shift gesture or Select All
    /// started, if it has not answered.
    func cancelSelectionRead() {
        selectionRead?.cancel()
        selectionRead = nil
    }

    /// Highlight the rows in view whose releases `kept` names, each from the
    /// row it draws. The coordinator sets `applyingSelection` meanwhile, so
    /// the delegate call that follows does not read the highlight as the
    /// person's change.
    func applySelection(_ kept: Set<String>) {
        let rows = highlightedRows(kept)
        guard rows != table.selectedRowIndexes else { return }
        applyingSelection = true
        table.selectRowIndexes(rows, byExtendingSelection: false)
        applyingSelection = false
    }

    /// The releases of the rows the table draws at `rows`. A row still
    /// loading names no release, and the coordinator logs it.
    private func ids(at rows: IndexSet) -> Set<String> {
        var ids = Set<String>()
        var loading: [Int] = []
        for row in rows {
            if let id = list.row(at: row)?.id {
                ids.insert(id)
            }
            else {
                loading.append(row)
            }
        }
        if !loading.isEmpty {
            logger.debug(
                "The selection skips rows \(loading), which are still loading"
            )
        }
        return ids
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
