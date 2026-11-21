import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacBox`, one shot for each row of its preview.
    static let macBoxScenes: [ShotScene] = [
        component("mac.component.box.empty", width: 580, height: 70) {
            ShotForm {
                MacSection {}.empty("No shared folders yet")
            }
        },
        component("mac.component.box.rows", width: 580, height: 160) {
            ShotBox {
                MacRow("This Mac") {}
                MacRow("Phone") {}
                MacRow("Laptop") {}
            }
        },
    ]
}
