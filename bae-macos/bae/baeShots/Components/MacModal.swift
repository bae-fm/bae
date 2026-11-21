import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacModal`, one shot for each row of its preview.
    static let macModalScenes: [ShotScene] = [
        component("mac.component.modal.default", width: 1_000, height: 260) {
            MacModal(onDismiss: {}) {
                MacModalHeader(title: "Storage") {
                    MacButton("Done") {}
                }
                .frame(width: 360, height: 200, alignment: .top)
                .background(Theme.background)
            }
            .frame(height: 260)
        }
    ]
}
