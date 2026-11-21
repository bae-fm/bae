import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAccentPicker`, one shot for each row of its preview.
    static let macAccentPickerScenes: [ShotScene] = [
        component("mac.component.accent-picker.resting", height: 60) {
            MacAccentPicker(selected: .pink, onSelect: { _ in })
        }
    ]
}
