import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPortField` in a row, and with a port the server can't use.
    static let macPortFieldScenes: [ShotScene] = [
        component("mac.component.port-field.port", height: 90) {
            ShotBox {
                LabeledContent("Port") {
                    MacPortField(value: "4533", onCommit: { _ in })
                }
            }
        },
        component("mac.component.port-field.error", height: 100) {
            ShotBox {
                SubsonicFieldRow(title: "Port") {
                    DangerNote("Another app is using this port")
                } control: {
                    MacPortField(
                        value: "4533",
                        invalid: true,
                        onCommit: { _ in }
                    )
                }
            }
        },
    ]
}
