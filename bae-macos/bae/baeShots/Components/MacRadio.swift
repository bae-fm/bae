import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacRadio`, checked and not, and with no note.
    static let macRadioScenes: [ShotScene] = [
        radio("mac.component.radio.checked", note: "Your Apple Account."),
        radio(
            "mac.component.radio.unchecked",
            note: "Your Apple Account.",
            checked: false
        ),
        radio("mac.component.radio.no-note"),
    ]

    private static func radio(
        _ id: String,
        note: String? = nil,
        checked: Bool = true
    ) -> ShotScene {
        component(id, width: 580, height: 100) {
            ShotBox {
                MacRadio(
                    title: Text("iCloud"),
                    note: note.map { Text($0) },
                    checked: checked
                )
            }
        }
    }
}
