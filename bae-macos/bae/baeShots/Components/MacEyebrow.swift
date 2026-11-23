import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacEyebrow`, one shot for each row of its preview.
    static let macEyebrowScenes: [ShotScene] = [
        component("mac.component.eyebrow.default", page: true, height: 50) {
            MacEyebrow(verbatim: "Side A")
        },
        component("mac.component.eyebrow.flush", page: true, height: 50) {
            MacEyebrow("Play on")
        },
    ]
}
