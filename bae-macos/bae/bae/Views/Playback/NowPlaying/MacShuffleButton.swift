import BaeKit
import SwiftUI

/// The shuffle control used in the transport and the queue heading.
struct MacShuffleButton: View {
    let isOn: Bool
    var glyph: ThemeIcon = .large
    let set: (Bool) -> Void

    var body: some View {
        MacIconButton(
            symbol: "shuffle",
            label: isOn ? "Turn off shuffle" : "Shuffle",
            active: isOn,
            iconSize: glyph
        ) {
            set(!isOn)
        }
    }
}
