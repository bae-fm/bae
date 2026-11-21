import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacWelcomeWindow`: the window's chrome with nothing in it, 600 by
    /// 260 as the preview draws it.
    static let macWelcomeWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.welcome-window.window",
            size: CGSize(width: 600, height: 260),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow(size: CGSize(width: 600, height: 260)) {
                    Color.clear
                }
            )
        }
    ]
}
