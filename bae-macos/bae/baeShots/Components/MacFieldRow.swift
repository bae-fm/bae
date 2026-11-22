import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFieldRow`, a boxed field with the label on its text line.
    static let macFieldRowScenes: [ShotScene] = [
        component("mac.component.field-row.field", height: 90) {
            ShotBox {
                MacFieldRow("Name") { nameField }
            }
        },
        component("mac.component.field-row.control-after", height: 90) {
            ShotBox {
                MacFieldRow("Name") {
                    nameField
                    Button("Set up") {}
                }
            }
        },
        component("mac.component.field-row.note", height: 110) {
            ShotBox {
                MacFieldRow("Name") {
                    VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                        nameField
                        FieldNote("People in this library see this name.")
                    }
                }
            }
        },
        component("mac.component.field-row.no-label", height: 90) {
            ShotBox {
                MacFieldRow("") {
                    BoxedField(placeholder: "Name", text: .constant(""))
                        .frame(width: 160)
                }
            }
        },
        component("mac.component.field-row.text-aligned", height: 130) {
            ShotBox {
                MacRow("Home music") {}
                MacFieldRow("") { nameField }
            }
        },
    ]

    private static var nameField: some View {
        BoxedField(text: .constant("Home music"))
            .frame(width: 160)
    }
}
