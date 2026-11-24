import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A sort pill's width, which its board sets from the boxes of its field, its
/// direction arrow and its remove glyph.
@MainActor
@Suite("Sort pill layout")
struct MacSortPillLayoutTests {
    @Test("a removable pill is as wide as its board draws it")
    func removablePillWidth() {
        let pill = MacSortPill(
            criterion: .constant(
                BridgeSortCriterion(field: .artist, direction: .ascending)
            ),
            takenFields: [],
            canRemove: true,
            onSetField: { _ in },
            onRemove: {}
        )
        let width = NSHostingView(rootView: pill).fittingSize.width
        // The board's pill: 12 pt each side round a 49.9 pt field, the 8.5 pt
        // arrow and the 8.5 pt remove glyph, 6 pt apart.
        let board: CGFloat = 102.9
        #expect(abs(width - board) < 1, "the pill is \(width) pt wide")
    }
}
