import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacIconButton`, one shot for each row of its preview.
    static let macIconButtonScenes: [ShotScene] = [
        component("mac.component.icon-button.resting", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
        },
        component("mac.component.icon-button.active", height: 60) {
            MacIconButton(symbol: "repeat", label: "Repeat", active: true) {}
        },
    ]
}
