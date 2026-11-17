import AppKit
import BaeKit
import Testing

@testable import bae

/// How the table's proposed selection changes the kept keys: the keys it
/// adds and takes out, the range whose rows core reads for a Shift-click, and
/// where Shift extends from.
///
/// The answer names only the keys a gesture changes, so a kept key the list
/// hides stays selected through every gesture but a plain click.
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
                == .change(.only([Self.row(2)]), anchor: Self.row(2))
        )
    }

    @Test("a click on empty space clears the selection")
    func clickOnNothingClears() {
        #expect(
            Self.answer(proposed: [], current: [1], gesture: .replace)
                == .change(.only([]), anchor: nil)
        )
    }

    @Test("a Command-click adds or takes out its row alone")
    func commandClickToggles() {
        #expect(
            Self.answer(
                proposed: [0, 1, 4],
                current: [0, 1],
                gesture: .toggle,
                target: 4
            )
                == .change(
                    .adjust(add: [Self.row(4)], remove: [], range: nil),
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
                    .adjust(add: [], remove: [Self.row(1)], range: nil),
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
    /// reads the rows between, the ones still loading included.
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
                    .adjust(
                        add: [
                            Self.row(2), Self.row(4), Self.row(8), Self.row(9),
                        ],
                        remove: [],
                        range: .init(from: Self.row(2), to: Self.row(9))
                    ),
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
                    .adjust(
                        add: [Self.row(8), Self.row(9)],
                        remove: [],
                        range: .init(from: Self.row(2), to: Self.row(9))
                    ),
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
                    .adjust(
                        add: [Self.row(1), Self.row(2)],
                        remove: [],
                        range: .init(from: Self.row(1), to: Self.row(2))
                    ),
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
                    .adjust(add: [], remove: [Self.row(2)], range: nil),
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
