import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacWelcomeActions`, the first choices on a Mac with no library.
    static let macWelcomeActionsScenes: [ShotScene] = [
        component(
            "mac.component.welcome-actions.resting",
            width: 244,
            height: 130
        ) {
            MacWelcomeActions(
                disabled: false,
                onCreate: {},
                onScanCode: {},
                onEnterCode: {}
            )
        }
    ]
}
