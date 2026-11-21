import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFormField`, one shot for each row of its preview: a label over a
    /// control, with a note under the control.
    static let macFormFieldScenes: [ShotScene] = [
        formField("mac.component.form-field.default", note: nil),
        formField(
            "mac.component.form-field.note",
            note: "Only you see this playlist."
        ),
    ]

    private static func formField(_ id: String, note: LocalizedStringKey?)
        -> ShotScene
    {
        component(id, height: 110) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text("Who sees it")
                MacPlaceholderLine(width: 388, height: 24)
                if let note {
                    FieldNote(note)
                }
            }
            .frame(width: 388, alignment: .leading)
        }
    }
}
