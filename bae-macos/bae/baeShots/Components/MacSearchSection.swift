import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSearchSection`, one shot for each row of its preview.
    static let macSearchSectionScenes: [ShotScene] = [
        component(
            "mac.component.search-section.default",
            page: true,
            height: 60
        ) {
            MacSearchSection("Albums")
                .frame(width: 380, alignment: .leading)
        }
    ]
}
