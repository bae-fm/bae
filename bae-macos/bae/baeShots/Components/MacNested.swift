import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacNested`: the Discogs key's field and the notice about it, under
    /// the switch they belong to.
    static let macNestedScenes: [ShotScene] = [
        component(
            "mac.component.nested.field-and-notice",
            width: 580,
            height: 220
        ) {
            ShotBox {
                SettingToggleRow(
                    title: "Search Discogs",
                    isOn: .constant(true)
                )
                MacNested {
                    DiscogsSettingsContent(
                        draft: .constant(""),
                        status: .rejected,
                        isEnabled: true,
                        isValidating: false,
                        wasRejected: false,
                        saveError: nil,
                        readError: nil,
                        onSave: {},
                        onRecheck: {}
                    )
                }
            }
        }
    ]
}
