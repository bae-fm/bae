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
/// key, read as the change it makes to the keys the screen keeps selected.
///
/// The table's own selection only proposes rows by position, some of them
/// headers or rows whose page has not landed, and this says what that
/// proposal means for the kept keys.
struct ImportListSelectionRequest {
    /// The rows the table proposes to have selected.
    let proposed: IndexSet
    /// The rows the table highlights now.
    let current: IndexSet
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

    /// A run of rows a Shift gesture spans, by the keys at its two ends:
    /// core reads every row between them, loaded or not.
    struct Run: Equatable {
        let from: Data
        let to: Data
    }

    /// A change to the kept keys. `ImportSelection.edit` makes it to the
    /// keys the selection holds when the change arrives.
    enum Edit: Equatable {
        /// Select exactly these keys.
        case only(Set<Data>)
        /// Add `add` and take out `remove`, keeping every other key, then
        /// add the rows of `range` once core reads them.
        case adjust(add: Set<Data>, remove: Set<Data>, range: Run?)
    }

    /// What the proposal asks of the kept keys.
    enum Answer: Equatable {
        /// It lands on nothing that can be selected: a header, a row still
        /// loading.
        case rejected
        /// It changes nothing.
        case unchanged
        /// Make `edit` to the kept keys, and extend from `anchor` next.
        case change(Edit, anchor: Data?)
    }

    var answer: Answer {
        let proposedKeys = Set(proposed.compactMap(key))
        guard proposed.isEmpty || !proposedKeys.isEmpty else {
            return .rejected
        }
        let highlighted = Set(current.compactMap(key))
        switch gesture {
        case .replace:
            return .change(
                .only(proposedKeys),
                anchor: target.flatMap(key)
                    ?? (proposedKeys.count == 1 ? proposedKeys.first : anchor)
            )
        case .toggle:
            let add = proposedKeys.subtracting(highlighted)
            let remove = highlighted.subtracting(proposedKeys)
            guard !add.isEmpty || !remove.isEmpty else { return .unchanged }
            return .change(
                .adjust(add: add, remove: remove, range: nil),
                anchor: target.flatMap(key) ?? anchor
            )
        case .extend:
            return extended(proposedKeys, highlighted: highlighted)
        }
    }

    /// A Shift gesture: the rows it proposes join the kept keys at once, and
    /// the run goes to core by its two ends, for every row between them,
    /// loaded or not. One that only shrinks the run takes out what it drops.
    private func extended(
        _ proposedKeys: Set<Data>,
        highlighted: Set<Data>
    ) -> Answer {
        let added = proposed.subtracting(current)
        guard let first = added.first, let last = added.last else {
            let remove = highlighted.subtracting(proposedKeys)
            guard !remove.isEmpty else { return .unchanged }
            return .change(
                .adjust(add: [], remove: remove, range: nil),
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
            .adjust(
                add: proposedKeys,
                remove: [],
                range: Run(from: from, to: to)
            ),
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
