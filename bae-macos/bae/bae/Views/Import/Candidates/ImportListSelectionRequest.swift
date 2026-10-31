import AppKit
import BaeKit

/// How a person pointed at rows of the list.
enum SelectionGesture {
    /// A plain click or arrow key: select exactly what it names.
    case replace
    /// Command held: add and take out, keeping the rest.
    case toggle
    /// Shift held: select the whole run from the anchor.
    case extend
}

extension SelectionGesture {
    /// The gesture the keys held down make of a change to the list's rows.
    init(_ modifiers: NSEvent.ModifierFlags) {
        if modifiers.contains(.command) {
            self = .toggle
        }
        else if modifiers.contains(.shift) {
            self = .extend
        }
        else {
            self = .replace
        }
    }
}

/// What the table's own selection handling proposed for a person's click or
/// key, read as the change it asks of core's selection.
///
/// The table keeps no selection of its own: it proposes rows by position,
/// some of them headers or rows whose page has not landed, and this says what
/// that proposal means for the selection core holds.
struct ImportListSelectionRequest {
    /// The rows the table proposes to have selected.
    let proposed: IndexSet
    /// The rows the table has selected now.
    let current: IndexSet
    /// The loaded rows shown selected now.
    let shown: Set<Data>
    let gesture: SelectionGesture
    /// The row the pointer pressed, for a click.
    let target: Int?
    /// The row the last gesture without Shift landed on, which a Shift
    /// gesture extends from when the run it proposes starts at a row whose
    /// page has gone.
    let anchor: Data?
    /// The candidate key the row at a position selects: `nil` for a group
    /// header and for a row whose page has not landed.
    let key: (Int) -> Data?

    /// What the proposal asks of core's selection.
    enum Answer: Equatable {
        /// It lands on nothing that can be selected: a header, a row still
        /// loading.
        case rejected
        /// It changes nothing.
        case unchanged
        /// Change the selection by `change`, show `shown` selected until core
        /// reflects it, and extend from `anchor` next.
        case change(
            BridgeSelectionChange,
            shown: Set<Data>,
            anchor: Data?
        )
    }

    var answer: Answer {
        let proposedKeys = Set(proposed.compactMap(key))
        guard proposed.isEmpty || !proposedKeys.isEmpty else {
            return .rejected
        }
        switch gesture {
        case .replace:
            // Sent even when the rows shown selected are the ones proposed:
            // core's selection may hold rows past the loaded pages too.
            return .change(
                .replace(keys: proposedKeys.inByteOrder),
                shown: proposedKeys,
                anchor: target.flatMap(key)
                    ?? (proposedKeys.count == 1 ? proposedKeys.first : anchor)
            )
        case .toggle:
            let add = proposedKeys.subtracting(shown)
            let remove = shown.subtracting(proposedKeys)
            guard !add.isEmpty || !remove.isEmpty else { return .unchanged }
            return .change(
                .toggle(add: add.inByteOrder, remove: remove.inByteOrder),
                shown: proposedKeys,
                anchor: target.flatMap(key) ?? anchor
            )
        case .extend:
            return extended(proposedKeys)
        }
    }

    /// A Shift gesture: the run it adds goes to core by its two ends, and
    /// core selects every row between them, loaded or not. One that only
    /// shrinks the selection takes out what it drops.
    private func extended(_ proposedKeys: Set<Data>) -> Answer {
        let added = proposed.subtracting(current)
        guard let first = added.first, let last = added.last else {
            let remove = shown.subtracting(proposedKeys)
            guard !remove.isEmpty else { return .unchanged }
            return .change(
                .toggle(add: [], remove: remove.inByteOrder),
                shown: proposedKeys,
                anchor: anchor
            )
        }
        let end = target ?? (last > (current.last ?? -1) ? last : first)
        guard let to = key(end),
            let run = proposed.rangeView.first(where: { $0.contains(end) })
        else { return .rejected }
        let start =
            end == run.upperBound - 1 ? run.lowerBound : run.upperBound - 1
        // The run's far end is where the selection already stood; its page
        // may have gone while the person scrolled to this end.
        guard let from = key(start) ?? anchor ?? firstKey(from: start, to: end)
        else { return .rejected }
        return .change(
            .extend(from: from, to: to),
            shown: proposedKeys,
            anchor: anchor ?? from
        )
    }

    /// The first row from `start` towards `end` that selects a candidate.
    private func firstKey(from start: Int, to end: Int) -> Data? {
        let step = start <= end ? 1 : -1
        return stride(from: start, through: end, by: step)
            .lazy
            .compactMap(key)
            .first
    }
}

extension Set<Data> {
    /// The keys in the order of their bytes, so the same keys always cross in
    /// one order.
    fileprivate var inByteOrder: [Data] {
        sorted { $0.lexicographicallyPrecedes($1) }
    }
}
