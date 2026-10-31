import AppKit
import BaeKit
import Testing

@testable import bae

/// What the table's proposed selection asks of core: the change, the rows
/// shown selected until core reflects it, and where Shift extends from.
@Suite("Import list selection request")
struct ImportListSelectionRequestTests {
    /// Rows 0 to 9 select `row-0` to `row-9`; row 3 is a header, and rows
    /// 5 to 7 have not loaded.
    private static func key(_ row: Int) -> Data? {
        guard row != 3, !(5...7).contains(row), (0..<10).contains(row) else {
            return nil
        }
        return Self.row(row)
    }

    /// The key row `index` selects.
    private static func row(_ index: Int) -> Data {
        Data("row-\(index)".utf8)
    }

    private static func answer(
        proposed: IndexSet,
        current: IndexSet,
        gesture: SelectionGesture,
        target: Int? = nil,
        anchor: Data? = nil
    ) -> ImportListSelectionRequest.Answer {
        ImportListSelectionRequest(
            proposed: proposed,
            current: current,
            shown: Set(current.compactMap(key)),
            gesture: gesture,
            target: target,
            anchor: anchor,
            key: key
        )
        .answer
    }

    @Test("a click selects its row alone, and extends from it next")
    func clickReplaces() {
        #expect(
            Self.answer(
                proposed: [2],
                current: [0, 1],
                gesture: .replace,
                target: 2
            )
                == .change(
                    .replace(keys: [Self.row(2)]),
                    shown: [Self.row(2)],
                    anchor: Self.row(2)
                )
        )
    }

    @Test("a click on empty space clears the selection")
    func clickOnNothingClears() {
        #expect(
            Self.answer(proposed: [], current: [1], gesture: .replace)
                == .change(.replace(keys: []), shown: [], anchor: nil)
        )
    }

    @Test("a Command-click adds or takes out its row")
    func commandClickToggles() {
        #expect(
            Self.answer(
                proposed: [0, 1, 4],
                current: [0, 1],
                gesture: .toggle,
                target: 4
            )
                == .change(
                    .toggle(add: [Self.row(4)], remove: []),
                    shown: [Self.row(0), Self.row(1), Self.row(4)],
                    anchor: Self.row(4)
                )
        )
        #expect(
            Self.answer(
                proposed: [0],
                current: [0, 1],
                gesture: .toggle,
                target: 1
            )
                == .change(
                    .toggle(add: [], remove: [Self.row(1)]),
                    shown: [Self.row(0)],
                    anchor: Self.row(1)
                )
        )
    }

    @Test("a header or a row still loading cannot be selected")
    func headersAndPlaceholdersAreRejected() {
        #expect(
            Self.answer(proposed: [3], current: [2], gesture: .replace)
                == .rejected
        )
        #expect(
            Self.answer(proposed: [6], current: [2], gesture: .replace)
                == .rejected
        )
    }

    /// The run a Shift-click proposes goes to core by its two ends; core
    /// selects the rows between, the ones still loading included.
    @Test("a Shift-click sends the two ends of the run it spans")
    func shiftClickSendsTheRunsEnds() {
        #expect(
            Self.answer(
                proposed: IndexSet(2...9),
                current: [2],
                gesture: .extend,
                target: 9,
                anchor: Self.row(2)
            )
                == .change(
                    .extend(from: Self.row(2), to: Self.row(9)),
                    shown: [
                        Self.row(2), Self.row(4), Self.row(8), Self.row(9),
                    ],
                    anchor: Self.row(2)
                )
        )
    }

    /// The row the run starts at may have scrolled out of the loaded pages;
    /// the run then starts where the last plain click was.
    @Test("a Shift-click from a row that has gone extends from the anchor")
    func shiftClickFromAnUnloadedRowUsesTheAnchor() {
        #expect(
            Self.answer(
                proposed: IndexSet(5...9),
                current: [],
                gesture: .extend,
                target: 9,
                anchor: Self.row(2)
            )
                == .change(
                    .extend(from: Self.row(2), to: Self.row(9)),
                    shown: [Self.row(8), Self.row(9)],
                    anchor: Self.row(2)
                )
        )
    }

    /// Shift with the down arrow adds the row below; with nothing added, it
    /// takes out what it drops.
    @Test("a Shift-arrow extends to the row it adds, or takes one out")
    func shiftArrow() {
        #expect(
            Self.answer(
                proposed: [1, 2],
                current: [1],
                gesture: .extend
            )
                == .change(
                    .extend(from: Self.row(1), to: Self.row(2)),
                    shown: [Self.row(1), Self.row(2)],
                    anchor: Self.row(1)
                )
        )
        #expect(
            Self.answer(
                proposed: [1],
                current: [1, 2],
                gesture: .extend,
                anchor: Self.row(1)
            )
                == .change(
                    .toggle(add: [], remove: [Self.row(2)]),
                    shown: [Self.row(1)],
                    anchor: Self.row(1)
                )
        )
    }

    @Test("Command and Shift held name their gestures")
    func modifiersNameTheGesture() {
        #expect(SelectionGesture([]) == .replace)
        #expect(SelectionGesture(.command) == .toggle)
        #expect(SelectionGesture(.shift) == .extend)
        #expect(SelectionGesture([.command, .shift]) == .toggle)
    }
}
