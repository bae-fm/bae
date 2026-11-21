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
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: ThemeSpace.line) {
                        Text("Port")
                        Text("Another app is using this port")
                            .themeText(.detail)
                            .foregroundStyle(Theme.danger)
                    }
                    Spacer(minLength: 0)
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
