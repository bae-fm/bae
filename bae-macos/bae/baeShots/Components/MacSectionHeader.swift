import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSectionHeader`, one shot for each row of its preview.
    static let macSectionHeaderScenes: [ShotScene] = [
        component(
            "mac.component.section-header.default",
            page: true,
            height: 70
        ) {
            MacSectionHeader(title: "Works")
                .frame(width: 380, alignment: .leading)
        }
    ]
}
