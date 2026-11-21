import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSection`, one shot for each row of its preview.
    static let macSectionScenes: [ShotScene] = [
        component("mac.component.section.default", page: true, height: 110) {
            ShotForm {
                MacSection("People") {
                    Text("The section’s content")
                }
            }
        }
    ]
}
