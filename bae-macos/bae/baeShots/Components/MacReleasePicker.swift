import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleasePicker` over two releases and over three.
    static let macReleasePickerScenes: [ShotScene] = [
        releasePicker(
            "mac.component.release-picker.two-releases",
            ["CD", "LP"],
            shown: 0
        ),
        releasePicker(
            "mac.component.release-picker.three-releases",
            ["CD", "LP", "Remaster"],
            shown: 1
        ),
    ]

    private static func releasePicker(
        _ id: String,
        _ names: [String],
        shown: Int
    ) -> ShotScene {
        component(id, width: 284, height: 60) {
            MacReleasePicker(
                detail: panelDetail(releases: names, shown: shown),
                currentReleaseId: nil,
                onSelect: { _ in }
            )
            .frame(width: 284, alignment: .leading)
        }
    }
}
