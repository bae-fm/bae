import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSwitch`, on and off.
    static let macSwitchScenes: [ShotScene] = [
        component("mac.component.switch.on", height: 50) {
            MacSwitch("Restore on launch", isOn: .constant(true))
        },
        component("mac.component.switch.off", height: 50) {
            MacSwitch("Restore on launch", isOn: .constant(false))
        },
    ]
}
