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
    ]

    private static var nameField: some View {
        TextField("", text: .constant("Home music"))
            .labelsHidden()
            .boxedTextField()
            .frame(width: 160)
    }
}
