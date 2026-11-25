import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacLibraryHeading`, one shot for each row of its preview.
    static let macLibraryHeadingScenes: [ShotScene] = [
        component(
            "mac.component.library-heading.controls",
            width: 580,
            height: 110
        ) {
            MacLibraryHeading(collapseProgress: 1, fullWidth: false) {
                // The preview's slot, as wide as its words: the heading
                // holds the sort controls as children it does not own.
                PreviewSlot("Group and sort")
                    .fixedSize()
            }
            .environment(UiStore())
        },
        // `MacLibraryHeading`, at the top of its list and scrolled.
        libraryHeading("mac.component.library-heading.top", collapse: 0),
        libraryHeading("mac.component.library-heading.scrolled", collapse: 1),
    ]

    private static func libraryHeading(
        _ id: String,
        collapse: Double
    ) -> ShotScene {
        component(id, width: 580, height: 110) {
            MacLibraryHeading(collapseProgress: collapse, fullWidth: false) {
                EmptyView()
            }
            .environment(UiStore())
        }
    }
}
