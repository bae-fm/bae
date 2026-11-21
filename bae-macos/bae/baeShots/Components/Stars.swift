import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Stars`, rated four and not rated.
    static let starsScenes: [ShotScene] = [
        component("mac.component.stars.rated", height: 40) {
            Stars(value: 4, size: 16)
        },
        component("mac.component.stars.not-rated", height: 40) {
            Stars(value: 0, size: 16)
        },
    ]
}
