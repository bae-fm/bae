import SwiftUI

@testable import bae

extension ShotScene {
    /// `Heart`, hearted and not hearted.
    static let heartScenes: [ShotScene] = [
        component("mac.component.heart.hearted", height: 40) {
            Heart(on: true, size: 16)
        },
        component("mac.component.heart.not-hearted", height: 40) {
            Heart(size: 16)
        },
    ]
}
