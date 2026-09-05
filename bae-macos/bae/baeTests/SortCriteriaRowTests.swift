import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A pill's field menu re-points its criterion in place. The list is where
/// that write lands, so the rule lives on the list.
@Suite("Re-pointing a sort pill")
struct SortCriteriaRowTests {
    @Test("a re-pointed criterion keeps its place and direction")
    func rePointingKeepsPlaceAndDirection() {
        var criteria = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending),
            BridgeSortCriterion(field: .title, direction: .descending),
        ]

        criteria.replaceField(.title, with: .artist)

        #expect(
            criteria == [
                BridgeSortCriterion(field: .dateAdded, direction: .descending),
                BridgeSortCriterion(field: .artist, direction: .descending),
            ]
        )
    }

    @Test("a field another pill sorts by is refused")
    func aTakenFieldIsRefused() {
        let before = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending),
            BridgeSortCriterion(field: .title, direction: .ascending),
        ]
        var criteria = before

        criteria.replaceField(.title, with: .dateAdded)
        criteria.replaceField(.title, with: .title)
        criteria.replaceField(.year, with: .artist)

        #expect(criteria == before)
    }

    /// The field name is a menu label. An AppKit menu button renders a label
    /// as its title and can drop the rest, so assert the pill still puts its
    /// field on screen.
    @MainActor
    @Test("the pill draws its field")
    func thePillDrawsItsField() async throws {
        let size = NSSize(width: 240, height: 44)
        let drawn = try await FindOnlineRendering.pixels(
            SortCriterionPill(
                criterion: .constant(
                    BridgeSortCriterion(
                        field: .dateAdded,
                        direction: .descending
                    )
                ),
                takenFields: [],
                canRemove: false,
                onSetField: { _ in },
                onRemove: {}
            ),
            size: size
        )
        let empty = try await FindOnlineRendering.pixels(
            Color.clear,
            size: size
        )

        #expect(drawn != empty)
    }
}
