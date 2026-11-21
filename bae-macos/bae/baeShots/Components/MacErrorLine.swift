import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacErrorLine`: one line, and a message that wraps.
    static let macErrorLineScenes: [ShotScene] = [
        component("mac.component.error-line.error", height: 50) {
            MacErrorLine("The pattern needs at least one value.")
        },
        component("mac.component.error-line.long-error", width: 580, height: 70)
        {
            MacErrorLine(
                "A name cannot contain “/” or “:”, because macOS uses them to separate folders. Take them out of the typed text."
            )
            .frame(maxWidth: .infinity, alignment: .leading)
        },
    ]
}
