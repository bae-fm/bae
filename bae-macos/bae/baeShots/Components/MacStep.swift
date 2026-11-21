import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStep`, one shot for each row of its preview.
    static let macStepScenes: [ShotScene] = [
        component("mac.component.step.with-a-code", width: 580, height: 376) {
            MacStep("Tell Ana this code") {
                MacMatchCode(
                    code: "412 778",
                    waiting: "Waiting for Ana to let you in…"
                )
            }
        }
    ]
}
