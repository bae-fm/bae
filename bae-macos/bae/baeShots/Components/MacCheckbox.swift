import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacCheckbox`, off and on, and with its label in the detail type.
    static let macCheckboxScenes: [ShotScene] = [
        component("mac.component.checkbox.off", height: 40) {
            MacCheckbox("Group by artist", isOn: .constant(false))
                .themeText(.body)
        },
        component("mac.component.checkbox.on", height: 40) {
            MacCheckbox("Group by artist", isOn: .constant(true))
                .themeText(.body)
        },
        component("mac.component.checkbox.detail", height: 40) {
            MacCheckbox(isOn: .constant(false), detail: true) {
                Text("Group by artist")
            }
        },
    ]
}
