import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacContentUnavailable`, one shot for each row of its preview.
    static let macContentUnavailableScenes: [ShotScene] = [
        component(
            "mac.component.content-unavailable.default",
            page: true,
            height: 200
        ) {
            MacContentUnavailable(title: "Composers")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    ]
}
