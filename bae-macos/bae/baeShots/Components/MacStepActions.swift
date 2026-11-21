import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStepActions`, one shot for each row of its preview.
    static let macStepActionsScenes: [ShotScene] = [
        component("mac.component.step-actions.two-buttons", height: 60) {
            MacStepActions {
                MacButton("Cancel") {}
                MacButton("Join", kind: .primary) {}
            }
            .frame(width: 380, alignment: .leading)
        }
    ]
}
