import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Stars`, rated four, not rated and as the number and one star.
    static let starsScenes: [ShotScene] = [
        component("mac.component.stars.rated", height: 40) {
            Stars(value: 4, size: 16)
        },
        component("mac.component.stars.not-rated", height: 40) {
            Stars(value: 0, size: 16)
        },
        component("mac.component.stars.number-and-one-star", height: 40) {
            Stars(value: 4, size: 16, showsNumber: true)
        },
    ]
}
