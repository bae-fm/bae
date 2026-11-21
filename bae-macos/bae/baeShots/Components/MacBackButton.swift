import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacBackButton`, back to the page before.
    static let macBackButtonScenes: [ShotScene] = [
        component("mac.component.back-button.default", page: true, height: 60) {
            MacBackButton(backTitle: "Playlists", onBack: {})
        }
    ]
}
