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
        playingFrom(
            "mac.component.queue-section.playing-from",
            shuffled: false
        ),
        playingFrom(
            "mac.component.queue-section.playing-from-shuffled",
            shuffled: true
        ),
        component("mac.component.queue-section.without-clear", height: 60) {
            MacQueueSection(
                title: "Up next",
                kind: .lane(MacQueueSection.Lane())
            )
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.queue-section.hang", height: 60) {
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
        component("mac.component.queue-section.wide", height: 60) {
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
    ]
    private static func playingFrom(_ id: String, shuffled: Bool) -> ShotScene {
        component(id, height: 60) {
            MacQueueSection(
                title: "Playing from · Low Tide Hours",
                kind: .lane(
                    MacQueueSection.Lane(
                        clear: MacQueueSection.Clear(
                            label: "Clear playing from",
                            action: {}
                        ),
                        shuffle: MacQueueSection.Shuffle(on: shuffled) { _ in }
                    )
                )
            )
            .frame(width: 380, alignment: .leading)
        }
    }

}
