import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFormHeading`, a ruled heading over a part of a form.
    static let macFormHeadingScenes: [ShotScene] = [
        component("mac.component.form-heading.heading", width: 580, height: 50)
        {
            MacFormHeading(title: "Release")
                .frame(width: 480)
        }
    ]
}
