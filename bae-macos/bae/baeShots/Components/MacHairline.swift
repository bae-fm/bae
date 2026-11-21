import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacHairline`, one shot for each row of its preview.
    static let macHairlineScenes: [ShotScene] = [
        component("mac.component.hairline.default", page: true, height: 60) {
            MacHairline()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    ]
}
