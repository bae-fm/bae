import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacDisclosure` in a settings form.
    static let macDisclosureScenes: [ShotScene] = [
        component("mac.component.disclosure.closed", height: 90) {
            ShotForm {
                MacDisclosure(isExpanded: .constant(false)) {
                } label: {
                    Text("Advanced")
                }
            }
        },
        component("mac.component.disclosure.open", height: 170) {
            ShotForm {
                MacDisclosure(isExpanded: .constant(true)) {
                    MacRow("Bit depth") {
                        MacPopUpButton("Bit depth", selection: .constant(16)) {
                            MacPopUpOption(verbatim: "16-bit", value: 16)
                        }
                        .fixedSize()
                    }
                    SettingToggleRow(
                        title: "Keep the pregap",
                        isOn: .constant(false)
                    )
                } label: {
                    Text("Advanced")
                }
            }
        },
    ]
}
