import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacModalHeader`, a sheet's title with its second line and button.
    static let macModalHeaderScenes: [ShotScene] = [
        component("mac.component.modal-header.default", width: 544, height: 80)
        {
            MacModalHeader(
                title: "Edit release",
                subtitle: "Low Tide Hours · CD"
            ) {
                Button("Close") {}
            }
        }
    ]
}
