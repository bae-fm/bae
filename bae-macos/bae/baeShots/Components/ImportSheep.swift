import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `ImportSheep`, one shot for each row of its preview: the sheep
    /// moving and held still.
    static let importSheepScenes: [ShotScene] = [
        component("mac.component.import-sheep.moving", height: 180) {
            ImportSheep(size: 140)
        },
        component("mac.component.import-sheep.still", height: 180) {
            ImportSheep(size: 140)
        },
    ]
}
