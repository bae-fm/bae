import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacNote`, a loose paragraph between boxes.
    static let macNoteScenes: [ShotScene] = [
        component("mac.component.note.default", page: true, height: 72) {
            MacNote(
                "A shared folder is required to invite people to the library."
            )
            .frame(width: 380, alignment: .leading)
        }
    ]
}
