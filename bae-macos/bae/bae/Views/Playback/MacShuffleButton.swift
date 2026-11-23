import BaeKit
import SwiftUI

/// The shuffle control used in the transport and the queue heading.
struct MacShuffleButton: View {
    /// The current shuffle mode and the command that sets it.
    struct Shuffle {
        let on: Bool
        let set: (Bool) -> Void
    }

    let shuffle: Shuffle
    var glyph: ThemeIcon = .large

    var body: some View {
        MacIconButton(
            symbol: "shuffle",
            label: shuffle.on ? "Turn off shuffle" : "Shuffle",
            active: shuffle.on,
            iconSize: glyph
        ) {
            shuffle.set(!shuffle.on)
        }
    }
}
