import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSortCriteria`, one pill per sort field.
    static let macSortCriteriaScenes: [ShotScene] = [
        component("mac.component.sort-criteria.one", height: 60) {
            MacSortCriteria(
                criteria: .constant([
                    BridgeSortCriterion(
                        field: .dateAdded,
                        direction: .descending
                    )
                ])
            )
        },
        component("mac.component.sort-criteria.two", height: 60) {
            MacSortCriteria(
                criteria: .constant([
                    BridgeSortCriterion(field: .artist, direction: .ascending),
                    BridgeSortCriterion(field: .year, direction: .descending),
                ])
            )
        },
    ]
}
