import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueSection`, a lane's heading.
    static let macQueueSectionScenes: [ShotScene] = [
        playingFrom("mac.component.queue-section.playing-from"),
        playingFrom(
            "mac.component.queue-section.playing-from-shuffled",
            shuffled: true
        ),
        playingFrom("mac.component.queue-section.without-clear", clear: false),
        playingFrom(
            "mac.component.queue-section.without-shuffle",
            shuffled: nil
        ),
        playingFrom("mac.component.queue-section.hang"),
        playingFrom("mac.component.queue-section.wide"),
    ]

    private static func playingFrom(
        _ id: String,
        shuffled: Bool? = false,
        clear: Bool = true
    ) -> ShotScene {
        component(id, height: 60) {
            MacQueueSection(
                title: "Playing from · Low Tide Hours",
                kind: .lane(
                    MacQueueSection.Lane(
                        clear: clear
                            ? MacQueueSection.Clear(
                                label: "Clear playing from",
                                action: {}
                            ) : nil,
                        shuffle: shuffled.map {
                            MacQueueSection.Shuffle(on: $0) { _ in }
                        }
                    )
                )
            )
            .frame(width: 380, alignment: .leading)
        }
    }
}
