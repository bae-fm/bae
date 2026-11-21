import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSpinner`, alone and with words.
    static let macSpinnerScenes: [ShotScene] = [
        component("mac.component.spinner.alone", height: 50) {
            MacSpinner()
        },
        component("mac.component.spinner.with-words", height: 50) {
            MacSpinner("Looking for receivers…")
        },
    ]
}
