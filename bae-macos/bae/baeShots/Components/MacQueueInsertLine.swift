import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueInsertLine`, where a dragged track will land.
    static let macQueueInsertLineScenes: [ShotScene] = [
        component(
            "mac.component.queue-insert-line.default",
            page: true,
            height: 40
        ) {
            MacQueueInsertLine().frame(width: 380)
        }
    ]
}
