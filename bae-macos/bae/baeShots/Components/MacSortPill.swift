import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSortPill` ascending, descending and removable, and
    /// `MacSortCriteria` with every field in use; `MacLibraryHeading` with
    /// its controls.
    static let macSortPillScenes: [ShotScene] = [
        sortPill("mac.component.sort-pill.up", .ascending),
        sortPill("mac.component.sort-pill.down", .descending),
        sortPill(
            "mac.component.sort-pill.removable",
            .descending,
            removable: true
        ),
    ]

    private static func sortPill(
        _ id: String,
        _ direction: BridgeSortDirection,
        removable: Bool = false
    ) -> ShotScene {
        component(id, height: 60) {
            MacSortPill(
                criterion: .constant(
                    BridgeSortCriterion(
                        field: .dateAdded,
                        direction: direction
                    )
                ),
                takenFields: [],
                canRemove: removable,
                onSetField: { _ in },
                onRemove: {}
            )
        }
    }
}
