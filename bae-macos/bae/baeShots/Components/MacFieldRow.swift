import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    private static let fieldLabelWidth: CGFloat = 90

    /// `MacFieldRow`, a boxed field with the label on its text line.
    static let macFieldRowScenes: [ShotScene] = [
        component("mac.component.field-row.field", height: 90) {
            ShotBox {
                MacFieldRow("Name", labelWidth: fieldLabelWidth) { nameField() }
            }
        },
        component("mac.component.field-row.control-after", height: 90) {
            ShotBox {
                MacFieldRow("Name", labelWidth: fieldLabelWidth) {
                    nameField()
                    MacButton("Set up", kind: .fieldAction) {}
                }
            }
        },
        component("mac.component.field-row.note", height: 110) {
            ShotBox {
                MacFieldRow("Name", labelWidth: fieldLabelWidth) {
                    VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                        nameField()
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
                MacFieldRow(aligningText: { nameField() })
            }
        },
    ]

    private static func nameField() -> some View {
        BoxedField(text: .constant("Home music"))
    }
}
