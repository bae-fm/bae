import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Logo`, the sheep over the word: at rest, and floating over its glow.
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
        component("mac.component.logo.inline", height: 100) {
            Logo(size: 80, bob: false, glow: false)
        },
        component("mac.component.logo.inline-right-to-left", height: 100) {
            Logo(size: 80, bob: false, glow: false)
        },
        component("mac.component.logo.sheep", height: 180) {
            Logo(size: 120, bob: false, glow: false)
        },
        component("mac.component.logo.word", height: 180) {
            Logo(size: 120, bob: false, glow: false)
        },
        component("mac.component.logo.small-32", height: 50) {
            Logo(size: 32, bob: false, glow: false)
        },
        component("mac.component.logo.small-16", height: 40) {
            Logo(size: 16, bob: false, glow: false)
        },
    ]
}
