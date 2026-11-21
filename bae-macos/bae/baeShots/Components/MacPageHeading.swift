import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPageHeading`, one shot for each row of its preview.
    static let macPageHeadingScenes: [ShotScene] = [
        component(
            "mac.component.page-heading.pane",
            width: 500,
            height: 70
        ) {
            MacPageHeading(title: Text("Library"))
        },
        component(
            "mac.component.page-heading.page-in-a-pane",
            width: 500,
            height: 70
        ) {
            MacPageHeading(
                title: Text("Sync"),
                back: .init(title: "Library", action: {})
            )
        },
    ]
}
