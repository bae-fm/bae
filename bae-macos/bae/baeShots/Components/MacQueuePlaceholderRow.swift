import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueuePlaceholderRow`, the skeleton of an entry that is loading.
    static let macQueuePlaceholderRowScenes: [ShotScene] = [
        component("mac.component.queue-placeholder-row.not-loaded", height: 80)
        {
            MacQueuePlaceholderRow()
                .frame(width: 380, alignment: .leading)
        }
    ]
}
