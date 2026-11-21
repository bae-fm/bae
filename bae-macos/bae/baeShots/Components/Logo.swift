import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Logo`, the sheep over the word: at rest, and floating over its glow.
    static let logoScenes: [ShotScene] = [
        component("mac.component.logo.pair", height: 180) {
            Logo(size: 120, bob: false, glow: false)
        }
    ]
}
