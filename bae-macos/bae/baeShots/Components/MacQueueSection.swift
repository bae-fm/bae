import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueSection`, a lane's heading or a day's.
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
        day("mac.component.queue-section.day"),
        day("mac.component.queue-section.day-with-menu", menu: true),
    ]

    /// A day of History, with the whose-plays menu that ends the first day
    /// when `menu` is true.
    private static func day(_ id: String, menu: Bool = false) -> ShotScene {
        component(id, height: 60) {
            MacQueueSection(
                title: "Playing from · Low Tide Hours",
                kind: .day(
                    menu: menu
                        ? historyMenu(person: nil)
                            .inlineMenu(action: false) { _ in } : nil
                )
            )
            .frame(width: 380, alignment: .leading)
        }
    }

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
