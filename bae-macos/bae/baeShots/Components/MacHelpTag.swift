import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacHelpTag`, one shot for each row of its preview: the tag showing
    /// under the control, and the control alone.
    static let macHelpTagScenes: [ShotScene] = [
        component("mac.component.help-tag.showing", height: 90) {
            helpTagged(showing: true)
        },
        component("mac.component.help-tag.hidden", height: 60) {
            helpTagged(showing: false)
        },
    ]

    private static func helpTagged(showing: Bool) -> some View {
        MacHelpTag(text: "On Ana’s Mac", showing: showing) {
            MacButton("Not on this device") {}
                .disabled(true)
        }
    }
}
