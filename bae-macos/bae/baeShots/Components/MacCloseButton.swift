import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacCloseButton`, one shot for each row of its preview.
    static let macCloseButtonScenes: [ShotScene] = [
        component("mac.component.close-button.close", height: 60) {
            MacCloseButton(onClose: {})
        }
    ]
}
