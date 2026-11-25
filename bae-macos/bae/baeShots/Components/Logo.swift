import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Logo`, the sheep over the word: at rest, and floating over its glow. The
    /// app draws no other form, so the preview's other rows draw for the design
    /// only.
    static let logoScenes: [ShotScene] = [
        component("mac.component.logo.pair", height: 180) {
            Logo(size: 120, bob: false, glow: false)
        },
        component("mac.component.logo.no-kind", height: 180) {
            Logo(size: 120, bob: false, glow: false)
        },
        component("mac.component.logo.pair-bob-and-glow", height: 180) {
            Logo(size: 120, bob: true, glow: true)
        },
    ]
}
