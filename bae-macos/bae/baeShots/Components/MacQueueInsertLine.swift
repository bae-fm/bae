import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueInsertLine`, where a dragged track will land.
    static let macQueueInsertLineScenes: [ShotScene] = [
        component(
            "mac.component.queue-insert-line.default",
            page: true,
            width: 968,
            height: 40
        ) {
            // The board's line runs the whole width of its preview row.
            MacQueueInsertLine().frame(width: 968)
                // Its board's line has a 1 point margin above it that
                // pulls it up, over the row above.
                .padding(.top, -1)
        }
    ]
}
