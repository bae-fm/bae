import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMenu`, one shot for each row of its preview.
    static let macMenuScenes: [ShotScene] = [
        component("mac.component.menu.open", height: 140) {
            MacMenuInPlace(
                {
                    MacMenuItem("Off") {}
                    MacMenuItem("Only me", checked: true) {}
                    MacMenuItem("Everyone") {}
                },
                highlighted: 1
            )
        },
        component("mac.component.menu.runs-past-the-screen", height: 140) {
            MacMenuInPlace(
                {
                    MacMenuItem("Off") {}
                    MacMenuItem("Only me", checked: true) {}
                    MacMenuItem("Everyone") {}
                },
                highlighted: 1
            )
        },
    ]
}
