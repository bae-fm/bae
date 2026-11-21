import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueSection`, a lane's heading.
    static let macQueueSectionScenes: [ShotScene] = [
        component("mac.component.queue-section.up-next", height: 60) {
            MacQueueSection(
                title: "Up next",
                kind: .lane(
                    MacQueueSection.Lane(
                        clear: MacQueueSection.Clear(
                            label: "Clear up next",
                            action: {}
                        )
                    )
                )
            )
            .frame(width: 380, alignment: .leading)
        },
        component(
            "mac.component.queue-section.playing-from-shuffled",
            height: 60
        ) {
            MacQueueSection(
                title: "Playing from · Low Tide Hours",
                kind: .lane(
                    MacQueueSection.Lane(
                        clear: MacQueueSection.Clear(
                            label: "Clear playing from",
                            action: {}
                        ),
                        shuffle: MacQueueSection.Shuffle(on: true) { _ in }
                    )
                )
            )
            .frame(width: 380, alignment: .leading)
        },
    ]
}
