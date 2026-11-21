import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacInfoTip`, one shot for each row of its preview.
    static let macInfoTipScenes: [ShotScene] = [
        component("mac.component.info-tip.after-a-row-name", height: 90) {
            ShotBox {
                SettingToggleRow(
                    title: "Search Discogs",
                    isOn: .constant(false)
                ) {
                    DiscogsInfoTip()
                }
            }
        }
    ]
}
