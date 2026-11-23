import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStepper`, named by what it counts.
    static let macStepperScenes: [ShotScene] = [
        component("mac.component.stepper.default", page: true, height: 40) {
            MacStepper(label: "Number of CD", value: .constant(1))
        },
        component("mac.component.stepper.off", page: true, height: 40) {
            MacStepper(label: "Number of CD", value: .constant(1), off: true)
        },
    ]
}
